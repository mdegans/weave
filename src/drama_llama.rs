use std::{path::PathBuf, sync::mpsc::TryRecvError};

use drama_llama::{Engine, PredictOptions};

/// A request to the [`Worker`] thread (from another thread).
#[derive(Debug)]
pub(crate) enum Request {
    /// The [`Worker`] should cancel the current generation.
    Stop,
    /// The [`Worker`] should continue the `text` with the given `opts`.
    Predict { text: String, opts: PredictOptions },
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

impl Worker {
    /// Start the worker thread. If the worker is already alive, this is a
    /// no-op. Use `restart` to restart the worker or change the model.
    ///
    /// This can return an error message if the model is not found or if an
    /// existing worker has returned an error.
    // FIXME: we can probably stop blocking altogether, but we'd have to change
    // a whole bunch of stuff and likely introduce async rumble jumble. It may
    // not be worth it since blocking is so rare. It only happens on shutdown or
    // model change, and only then in the middle of an inference.
    pub fn start(
        &mut self,
        model: PathBuf,
        context: egui::Context,
    ) -> Result<(), std::io::Error> {
        // Loading is impossible
        if !model.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Model not found",
            ));
        }

        // If the worker is already alive, do nothing.
        if self.is_alive() {
            log::debug!("Worker is already alive");
            return Ok(());
        }
        log::debug!("Starting `drama_llama` worker thread.");

        // Create channels to and from the worker from the (probably) main
        // thread.
        let (to_worker, from_main) = std::sync::mpsc::channel();
        let (to_main, from_worker) = std::sync::mpsc::channel();

        // Spawn the actual worker thread.
        let handle = std::thread::spawn(move || {
            // FIXME: the error types in `drama_llama` are now all Send, so we
            // can return any error types.
            // FIXME: the Args are not Clone or Default but they should be. Also
            // they are not necessarily cli specific so the code in drama_llama
            // should be refactored to be more general rather than requiring
            // the `cli` feature, and clap, for the Args struct.
            let args = drama_llama::cli::Args {
                model: model.clone(),
                context: 512,
                no_gpu: false,
                vocab: drama_llama::VocabKind::Unsafe,
            };
            log::info!("Loading `Engine` with `Args`: {:#?}", args);
            let mut engine = Engine::from_cli(args, None).unwrap();

            while let Ok(msg) = from_main.recv() {
                let (text, opts) = match msg {
                    Request::Stop => {
                        to_main.send(Response::Done).ok();
                        context.request_repaint();
                        break;
                    }
                    Request::Predict { text, opts } => {
                        // If the requested context size is greater than the
                        // engine's we must recreate it.
                        if opts.n.get() > engine.n_ctx() as usize {
                            let args = drama_llama::cli::Args {
                                model: model.clone(),
                                context: 512.max(opts.n.get() as u32),
                                no_gpu: false,
                                vocab: drama_llama::VocabKind::Unsafe,
                            };
                            log::info!(
                                "Recreating engine with context size: {}",
                                args.context
                            );
                            engine = Engine::from_cli(args, None).unwrap();
                        }
                        (text, opts)
                    }
                };

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

    /// Launch a prediction. If the worker is not alive, or the channel is
    /// closed, return an error. Does not block.
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

    /// Try to receive the next piece of text from the worker. If the worker is
    /// not alive, this returns None. If the channel is empty or closed,
    /// Some(error) is returned.
    pub fn try_recv(&self) -> Option<Result<Response, TryRecvError>> {
        self.from_worker
            .as_ref()
            .map(|from_worker| from_worker.try_recv())
    }
}
