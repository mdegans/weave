use std::{
    collections::BTreeMap, num::NonZeroUsize, path::PathBuf,
    sync::mpsc::TryRecvError,
};

use drama_llama::{Engine, PredictOptions};

/// A request to the [`Worker`] thread (from another thread).
#[derive(Debug)]
pub(crate) enum Request {
    /// The [`Worker`] should cancel the current generation.
    Stop,
    /// The [`Worker`] should continue the `text` with the given `opts`.
    Predict { text: String, opts: PredictOptions },
    /// A new model should be loaded.
    LoadModel { model: PathBuf },
}

/// A response from the [`Worker`] thread (to another thread).
#[derive(Debug)]
pub(crate) enum Response {
    /// [`Worker`] is done and can accept new requests.
    Done,
    /// The [`Worker`] is busy and cannot accept new requests.
    Busy { request: Request },
    /// The [`Worker`] has predicted a piece of text.
    Predicted { piece: String },
    /// The [`Worker`] has encountered an error.
    Error { error: Error },
    /// The [`Worker`] has loaded a new model.
    LoadedModel {
        /// The path to the new model.
        model: PathBuf,
        /// Maximum context size supported by the model.
        max_context_size: usize,
        /// Model metadata.
        metadata: BTreeMap<String, String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Error loading model: {error}")]
    LoadError {
        #[from]
        error: drama_llama::NewError,
    },
    #[error("Cannot predict with no model loaded.")]
    NoModelLoaded { request: Request },
    #[error(
        "Context size {requested} is too large for model. Maximum supported: {supported}"
    )]
    ContextTooLarge { requested: usize, supported: usize },
    #[error("Worker thread is dead.")]
    WorkerDead {
        #[from]
        source: WorkerDead,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkerDead {
    #[error(
        "Can't send request because worker thread is dead. Worker may have panicked."
    )]
    Send {
        #[from]
        source: std::sync::mpsc::SendError<Request>,
    },
    #[error(
        "Can't receive response because worker thread is dead. Worker may have panicked."
    )]
    Recv {
        #[from]
        source: TryRecvError,
    },
}

/// A worker helps to manage the `drama_llama` worker thread and its channels.
#[derive(Default)]
pub(crate) struct Worker {
    /// Thread handle to the worker.
    handle: Option<std::thread::JoinHandle<()>>,
    /// Channel to send text and options to the worker.
    to_worker: Option<std::sync::mpsc::Sender<Request>>,
    /// Channel to receive strings until the worker is done, then `None`.
    from_worker: Option<std::sync::mpsc::Receiver<Response>>,
}

fn load_model(model: PathBuf, ctx: usize) -> Result<Engine, Error> {
    let args = drama_llama::cli::Args {
        model: model.clone(),
        context: 512.max(ctx as u32),
        no_gpu: false,
        // We're not outputing to the web or anything that parses code so we
        // can use the unsanitized vocab. LLaMA 3 kind of broke this feature
        // anyway since it has a completely different tokenizer.
        vocab: drama_llama::VocabKind::Unsafe,
    };
    log::info!("Creating engine with context size: {}", args.context);

    // FIXME: this logic should be in drama_llama, not here.
    let engine = Engine::from_cli(args, None)?;
    let supported = engine.model.context_size().try_into().unwrap_or(0);
    if ctx > supported {
        return Err(Error::ContextTooLarge {
            requested: ctx,
            supported,
        });
    }

    Ok(engine)
}

impl Worker {
    /// Start the worker thread. If the worker is already alive, this is a
    /// no-op. Use `restart` to restart the worker or change the model.
    ///
    /// This can return an error message if the model is not found or if an
    /// existing worker has returned an error.
    pub fn start(
        &mut self,
        context: egui::Context,
    ) -> Result<(), std::io::Error> {
        // If the worker is already alive, do nothing.
        if self.is_alive() {
            log::error!("Worker is already alive");
            return Ok(());
        }

        // Get the number of threads available to the system.
        let n_threads: u32 = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(1).unwrap())
            .get()
            .try_into()
            .unwrap_or(1);

        // Create channels to and from the worker from the (probably) main
        // thread.
        let (to_worker, from_main) = std::sync::mpsc::channel();
        let (to_main, from_worker) = std::sync::mpsc::channel();

        // Spawn the actual worker thread.
        log::debug!("Starting `drama_llama` worker thread.");
        let handle = std::thread::spawn(move || {
            let mut model_path = None;
            let mut engine = None;
            while let Ok(msg) = from_main.recv() {
                let (text, opts) = match msg {
                    Request::Stop => {
                        // We're done with this generation. Generally this is
                        // handled in the tight loop below, but we need to
                        // handle it here too in case the main thread sends a
                        // stop command just as we finish a piece.
                        to_main.send(Response::Done).ok();
                        context.request_repaint();
                        continue;
                    }
                    Request::LoadModel { model } => {
                        // clear any existing engine before we load a new one or
                        // we can get errors about memory usage, because the old
                        // engine is still holding onto memory.
                        drop(engine.take());
                        engine = match load_model(model.clone(), 512) {
                            Ok(engine) => {
                                // Model loaded successfully.
                                model_path = Some(model.clone());
                                log::info!(
                                    "Model loaded: {:?}",
                                    model
                                        .file_name()
                                        .unwrap()
                                        .to_string_lossy()
                                );

                                // Let the main thread know we've loaded the
                                // model.
                                let metadata = engine.model.meta();
                                log::info!("Model Metadata: {:#?}", &metadata);
                                let max_context_size = engine
                                    .model
                                    .context_size()
                                    .try_into()
                                    .unwrap_or(0);
                                log::info!(
                                    "Model context size: {}",
                                    max_context_size
                                );

                                to_main
                                    .send(Response::LoadedModel {
                                        model,
                                        max_context_size,
                                        metadata,
                                    })
                                    .ok();

                                // We have an Engine now.
                                Some(engine)
                            }
                            Err(e) => {
                                to_main
                                    .send(Response::Error { error: e.into() })
                                    .ok();
                                model_path = None;
                                None // clear any existing engine
                            }
                        };
                        continue;
                    }
                    Request::Predict { text, opts } => {
                        // If the requested context size is greater than the
                        // engine's we must recreate it. We must take it because
                        // we may need to drop it.
                        if let Some(e) = engine.take() {
                            if opts.n.get() > e.n_ctx().max(512) as usize {
                                // Drop the engine before we load a new one
                                // because it's holding onto memory.
                                drop(e);
                                (engine, model_path) = match load_model(
                                    // if we have an engine, we have a model
                                    // path so this can't be None
                                    model_path.clone().unwrap(),
                                    opts.n.get(),
                                ) {
                                    Ok(e) => (Some(e), model_path),
                                    Err(e) => {
                                        to_main
                                            .send(Response::Error {
                                                error: e.into(),
                                            })
                                            .ok();
                                        // We don't have an engine now, so we
                                        // don't have a model path either.
                                        (None, None)
                                    }
                                };
                            } else {
                                // Put the engine back if we didn't recreate it.
                                engine = Some(e);
                            }
                        };

                        if engine.is_none() {
                            // We can't satisfy the prediction request for this
                            // model and requested context size.
                            to_main
                                .send(Response::Error {
                                    error: Error::NoModelLoaded {
                                        request: Request::Predict {
                                            text,
                                            opts,
                                        },
                                    },
                                })
                                .ok();
                            continue;
                        }

                        (text, opts)
                    }
                };

                // We can unwrap here because we've already checked that the
                // engine is not None.
                let engine = engine.as_mut().unwrap();

                // Configure the engine to use all available threads. LLaMA.cpp
                // may run some layers on the CPU if they are not supported by
                // the GPU, so this is useful.
                engine.set_n_threads(n_threads, n_threads);

                // Add any model-specific stop criteria. We do want to check
                // here rather than add it to the settings because if the user
                // changes model, the tokens will be different, but still in the
                // stop criteria, which would result in unexpected behavior.
                let opts = opts.add_model_stops(&engine.model);

                // Tokenize the text, predict pieces, and send them back.
                let tokens = engine.model.tokenize(&text, true);
                for piece in engine.predict_pieces(tokens, opts) {
                    // We check every token for a stop or disconnect signal
                    // since it is the tightest loop we have.
                    match from_main.try_recv() {
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // No new requests, nothing to do.
                        }
                        Ok(Request::Stop) => {
                            log::debug!("Generation cancelled.");
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // Main thread has dropped the channel. This is our
                            // cue to exit.
                            return;
                        }
                        Ok(command) => {
                            // We can't handle this command right now. We'll
                            // send a busy Response and the main thread can
                            // decide what to do.
                            to_main
                                .send(Response::Busy { request: command })
                                .ok();
                            context.request_repaint();
                        }
                    }

                    // Send the predicted piece back to the main thread.
                    to_main.send(Response::Predicted { piece }).ok();
                    context.request_repaint();
                }

                // We are ready for the next command.
                to_main.send(Response::Done).ok();
                // When we're done we should repaint the UI, but we need to make
                // sure the main thread has time to process the message first
                // or we'll redraw before the last token is added. 100ms should
                // be enough time.
                context.request_repaint();
                context.request_repaint_after(
                    std::time::Duration::from_millis(100),
                );
            }
        });

        self.handle = Some(handle);
        self.to_worker = Some(to_worker);
        self.from_worker = Some(from_worker);

        Ok(())
    }

    /// Stop current generation after the next token. Does not shut down the
    /// worker thread. Does not block. Does not guarantee that generation will
    /// stop immediately. Use [`Worker::shutdown`] to shut down the worker.
    pub fn stop(&mut self) -> Result<(), std::sync::mpsc::SendError<Request>> {
        log::debug!("Telling worker to cancel current generation.");
        if let Some(to_worker) = self.to_worker.as_ref() {
            to_worker.send(Request::Stop)?;
        }

        Ok(())
    }

    /// Shutdown the worker thread. If the worker is not alive, this is a no-op.
    ///
    /// This will block until the worker is done (the next piece is yielded).
    /// This can return an error if the worker thread panics.
    pub fn shutdown(
        &mut self,
    ) -> Result<(), Box<dyn std::any::Any + Send + 'static>> {
        log::debug!("Shutting down `drama_llama` worker thread.");
        if let Some(to_worker) = self.to_worker.take() {
            // Trigger the worker to shut down on the next piece. Dropping the
            // channel disconnects the worker and breaks it's main loop.
            log::debug!("Telling worker to stop.");
            drop(to_worker);
        }

        let mut ret = Ok(());

        if let Some(handle) = self.handle.take() {
            log::debug!("Waiting for worker to finish.");
            ret = handle.join();
            log::debug!("Worker has finished.");
        }

        self.from_worker = None;
        self.to_worker = None;

        log::debug!("Worker has been shut down.");
        ret
    }

    /// Returns true if the worker is alive.
    pub fn is_alive(&self) -> bool {
        self.handle.is_some()
    }

    /// Load a new model. If the worker is not alive, *or the channel is closed
    /// (the worker is shutting down)*, return an error.
    ///
    /// Does not block.
    pub fn load_model(
        &mut self,
        model: PathBuf,
    ) -> Result<(), std::sync::mpsc::SendError<Request>> {
        if !self.is_alive() {
            return Err(std::sync::mpsc::SendError(Request::LoadModel {
                model,
            }));
        }

        if let Some(to_worker) = self.to_worker.as_ref() {
            to_worker.send(Request::LoadModel { model })?;
        } else {
            return Err(std::sync::mpsc::SendError(Request::LoadModel {
                model,
            }));
        }

        Ok(())
    }

    /// Launch a prediction. If the worker is not alive, *or the channel is
    /// closed (the worker is shutting down)*, return an error.
    ///
    /// Does not block.
    pub fn predict(
        &mut self,
        text: String,
        options: drama_llama::PredictOptions,
    ) -> Result<(), std::sync::mpsc::SendError<Request>> {
        if !self.is_alive() {
            return Err(std::sync::mpsc::SendError(Request::Predict {
                text,
                opts: options,
            }));
        }

        if let Some(to_worker) = self.to_worker.as_ref() {
            to_worker.send(Request::Predict {
                text,
                opts: options,
            })?;
        } else {
            return Err(std::sync::mpsc::SendError(Request::Predict {
                text,
                opts: options,
            }));
        }

        Ok(())
    }

    /// Send a [`Request`] to the worker. If the worker is not alive, *or the
    /// channel is closed (the worker is shutting down)*, return an error.
    ///
    /// Does not block.
    pub fn send(
        &mut self,
        request: Request,
    ) -> Result<(), std::sync::mpsc::SendError<Request>> {
        if !self.is_alive() {
            return Err(std::sync::mpsc::SendError(request));
        }

        if let Some(to_worker) = self.to_worker.as_ref() {
            to_worker.send(request)?;
        } else {
            return Err(std::sync::mpsc::SendError(request));
        }

        Ok(())
    }

    /// Try to receive the next response or error from the worker. If the worker
    /// is not alive or there is no message, this returns None. If the worker
    /// has just died, this will return an error and shut down the worker.
    ///
    /// Does not block.
    pub fn try_recv(&mut self) -> Option<Result<Response, Error>> {
        let mut shutdown = false;
        let ret = match &self.from_worker {
            Some(channel) => match channel.try_recv() {
                Ok(Response::Error { error }) => Err(error),
                Ok(reponse) => Ok(reponse),
                Err(e) => {
                    if let TryRecvError::Disconnected = e {
                        shutdown = true;
                        Err(WorkerDead::from(e).into())
                    } else {
                        // No message, nothing to do.
                        return None;
                    }
                }
            },
            // No channel, nothing to do.
            None => return None,
        };

        if shutdown {
            self.shutdown().ok();
        }

        Some(ret)
    }
}
