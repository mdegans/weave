use std::panic;

use futures::SinkExt;
use serde::{Deserialize, Serialize};
// TODO: This crate does not support third-party endpoints. We should fix this
// and send a PR or use another crate. It would be nice to support local models
// indirectly, even though they are directly supported by `drama_llama`.
use openai_rust::{chat::Message, Client};

#[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(remote = "openai_rust::chat::ChatArguments")]
pub struct ChatArguments {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub n: Option<u32>,
    #[serde(default)]
    pub stop: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub user: Option<String>,
}

impl Into<openai_rust::chat::ChatArguments> for ChatArguments {
    fn into(self) -> openai_rust::chat::ChatArguments {
        // nope We can't do this because of private fields!
        // openai_rust::chat::ChatArguments {
        //     model: self.model,
        //     messages: self.messages,
        //     temperature: self.temperature,
        //     top_p: self.top_p,
        //     n: self.n,
        //     stream: self.stream.unwrap_or(false),
        //     stop: self.stop,
        // }
        let mut args =
            openai_rust::chat::ChatArguments::new(self.model, self.messages);
        args.temperature = self.temperature;
        args.top_p = self.top_p;
        args.n = self.n;
        args.stop = self.stop;
        args.max_tokens = self.max_tokens;
        args.presence_penalty = self.presence_penalty;
        args.frequency_penalty = self.frequency_penalty;
        args.user = self.user;

        args
    }
}

impl ChatArguments {
    pub fn ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        // `model` is set by the parent `Settings` struct, since it has the
        // available choices. We don't draw it here.

        // `messages`
        let mut delete = Vec::new();
        let mut ret = ui.label("Examples").on_hover_text_at_pointer("This is a list of messages that will be sent to the chat API to bootstrap the conversation. For more info, see: https://cookbook.openai.com/examples/how_to_format_inputs_to_chatgpt_models");
        for (i, example) in
            self.messages.iter_mut().enumerate()
        {
            ret |= ui.horizontal(|ui| {
                if ui.button("❌").clicked() {
                    delete.push(i);
                }
                // NOTE: OpenAI no longer requires the role to be in a
                // specific order. We don't need to enforce any alternation
                // between user and assistant or even that `system` can't
                // chime in. There is also now a `tool` role. Best we leave
                // this as flexible as possible.
                ui.vertical(|ui| {
                    ui.text_edit_singleline(&mut example.role).on_hover_text_at_pointer("Generally the role should be: `user`, `assistant`, `system`, or `tool`. Some third-party OpenAI compaitble endpoints may not care about this or may have additional roles.")
                    | ui.text_edit_multiline(&mut example.content)
                })
            }).response;
        }

        if ui.button("Add Message").clicked() {
            let role = self
                .messages
                .last()
                .map(|m| match m.role.as_str() {
                    "user" => "assistant",
                    "assistant" | "system" => "user",
                    _ => "user",
                })
                .unwrap_or("system");

            self.messages.push(Message {
                role: role.to_string(),
                content: "".to_string(),
            });
        }

        // Filter out any examples that were deleted.
        if !delete.is_empty() {
            self.messages = self
                .messages
                .drain(..)
                .enumerate()
                .filter_map(|(i, example)| {
                    if delete.contains(&i) {
                        None
                    } else {
                        Some(example)
                    }
                })
                .collect();
        }

        // `temperature` should be a slider from 0.0 to 1.0.
        let temperature = self.temperature.get_or_insert(1.0);
        ret |= ui.add(
            egui::Slider::new(temperature, 0.0..=1.0)
                .text("Temperature")
                .clamp_to_range(true),
        ).on_hover_text_at_pointer("How creative the model is. 0.0 is very conservative, 1.0 is very creative. OpenAI's default is 1.0.");

        // `top_p` should be a slider from 0.0 to 1.0.
        let top_p = self.top_p.get_or_insert(1.0);
        ret |= ui.add(
            egui::Slider::new(top_p, 0.0..=1.0)
                .text("Top P")
                .clamp_to_range(true),
        ).on_hover_text_at_pointer("The cumulative probability of the model's output. 0.0 is very conservative, 1.0 is very creative. OpenAI's default is 1.0. Use this or `temperature`, not both.");

        // Stop on newline. The OpenAI API itself supports multiple stop strings
        // but the crate does not. We can add other stop criteria later. For now
        // we need to use the single stop string for newline. We also can't use
        // the text edit field since it escapes `\n` as `\\n`.
        let mut stop_at_newline = self.stop.as_ref().is_some_and(|s| s == "\n");
        if ui.toggle_value(&mut stop_at_newline, "Stop at newline").changed() {
            if stop_at_newline {
                self.stop = Some("\n".to_string());
            } else {
                self.stop = None;
            }
        };

        // `max_tokens` should be a slider from 1 to 128000, which is the max
        // context for GPT-4o. This can possibly be even higher since models
        // keep getting more advanced. Realistically, it should be set to
        // something like 1024 since we want to generate paragraphs, not
        // entire books.
        ret |= ui.horizontal(|ui|{
            let max_tokens = self.max_tokens.get_or_insert(1024);

            ui.label("Max Tokens") |
            ui.add(
                egui::DragValue::new(max_tokens).clamp_range(1..=128000),
            )
        }).inner.on_hover_text_at_pointer("The maximum number of tokens to generate. OpenAI's default is 1024.");

        // `presence_penalty` should be a slider from 0.0 to 1.0.
        let presence_penalty = self.presence_penalty.get_or_insert(0.0);
        ret |= ui.add(
            egui::Slider::new(presence_penalty, -2.0..=2.0)
                .text("Presence Penalty")
                .clamp_to_range(true),
        ).on_hover_text_at_pointer("How much the model should avoid repeating itself. 0.0 is no penalty, 2.0 is maximum penalty. Negative numbers are not recommended. OpenAI's default is 0.0.");

        // `frequency_penalty` should be a slider from 0.0 to 1.0.
        let frequency_penalty = self.frequency_penalty.get_or_insert(0.0);
        ret |= ui.add(
            egui::Slider::new(frequency_penalty, -2.0..=2.0)
                .text("Frequency Penalty")
                .clamp_to_range(true),
        ).on_hover_text_at_pointer("How much the model should avoid repeating itself. 0.0 is no penalty, 2.0 is maximum penalty. Negative numbers are not recommended. OpenAI's default is 0.0.");

        // `user` is a text field specifying the user ID. We can set this from
        // the granparent that has the author name. It's not required but it's
        // not a bad idea to set it.

        ret
    }
}

impl Into<ChatArguments> for openai_rust::chat::ChatArguments {
    fn into(self) -> ChatArguments {
        ChatArguments {
            model: self.model,
            messages: self.messages,
            temperature: self.temperature,
            top_p: self.top_p,
            n: self.n,
            stop: self.stop,
            max_tokens: self.max_tokens,
            presence_penalty: self.presence_penalty,
            frequency_penalty: self.frequency_penalty,
            user: self.user,
        }
    }
}

impl Default for ChatArguments {
    fn default() -> Self {
        // This is available to all users. GPT-4 requires at least $5 credit to
        // use.
        const DEFAULT_MODEL: &str = "gpt-3.5-turbo";
        Self {
            model: DEFAULT_MODEL.to_string(),
            messages: vec![Message {
                role: "system".to_string(),
                content: "A user and an assistant are collaborating on a story. The user starts by writing a paragraph, then the assistant writes a paragraph, and so on. Both will be credited for the end result.'"
                    .to_string(),
            },Message {
                role: "user".to_string(),
                content: "Hi, GPT! Let's write a story together."
                    .to_string(),
            },Message {
                role: "assistant".to_string(),
                content: "Sure, I'd love to help. How about you start us off? I'll try to match your tone and style."
                    .to_string(),
            }],
            temperature: None,
            top_p: None,
            n: None,
            stop: None,
            max_tokens: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        }
    }
}

/// Fake deserializer for the api key. This will avoid saving the api key in
/// plain text in the settings file. It will use the keyring to store the key
/// instead.
fn get_api_key<'de, D>(_deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use keyring::Entry;

    let _ = String::deserialize(_deserializer);

    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        log::warn!("Using OPENAI_API_KEY environment variable is not secure, even though everybody does it.");
        // Because it can be logged or otherwise exposed. But we can use it to
        // initialize the keyring.
        return Ok(key);
    }

    match Entry::new("weave", "openai_api_key") {
        Ok(entry) => match entry.get_password() {
            Ok(key) => Ok(key),
            Err(e) => {
                log::error!("Couldn't get OpenAI API key because: {}", e);
                // In this case we default to an empty string. This is not
                // exactly deserializing, but it's the behavior we want.
                return Ok("".to_string());
            }
        },
        Err(e) => {
            log::error!("Couldn't get OpenAI API key because: {}", e);
            return Ok("".to_string());
        }
    }
}

/// Fake serializer for the api key. This will avoid saving the api key in
/// plain text in the settings file. It will use the keyring to store the key
/// instead.
fn set_api_key<S>(api_key: &String, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use keyring::Entry;

    let ret = serializer.serialize_str("hidden in keyring");

    if api_key.is_empty() {
        return ret;
    }

    match Entry::new("weave", "openai_api_key") {
        Ok(entry) => match entry.set_password(api_key) {
            Ok(()) => ret,
            Err(e) => {
                log::error!("Couldn't set OpenAI API key because: {}", e);
                // In this case we default to an empty string. This is not
                // exactly deserializing, but it's the behavior we want.
                ret
            }
        },
        Err(e) => {
            log::error!("Couldn't set OpenAI API key because: {}", e);
            ret
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Available models, if available from the OpenAI API. We don't want to
    /// serialize or deserialize this, because it changes. Call `fetch_models`
    /// after deserializing to populate this.
    #[serde(skip)]
    pub(crate) models: Vec<openai_rust::models::Model>,
    /// OpenAI API key
    #[serde(deserialize_with = "get_api_key", serialize_with = "set_api_key")]
    pub(crate) openai_api_key: String,
    /// Chat arguments
    pub(crate) chat_arguments: ChatArguments,
}

impl Settings {
    pub fn new(api_key: String, chat_arguments: ChatArguments) -> Self {
        Self {
            models: Vec::new(),
            openai_api_key: api_key,
            chat_arguments,
        }
    }

    /// Available models, if fetched from the OpenAI API.
    pub fn models(&self) -> &Vec<openai_rust::models::Model> {
        &self.models
    }

    /// Fetch the models from the OpenAI API synchronously. This will overwrite
    /// the current models. If a client is not provided, a new one will be
    /// created using the OpenAI API key.
    ///
    /// This blocks the current thread. Use this only on startup or when such
    /// blocking is acceptable.
    // Ugh. `Client` is not clone even though it just wraps a reqwest client
    // which is clone and has to be clone. Bluh. Another reason to look for
    // another crate.
    pub fn fetch_models_sync(
        &mut self,
        client: Option<&Client>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // FIXME: The `openai_rust` crate demands tokio use. There are too many
        // issues with the crate. We should change it and we can rip out a lot
        // of useless dependencies.
        tokio::runtime::Runtime::new().unwrap().block_on(self.fetch_models(client))
    }

    /// Fetch the models from the OpenAI API. This will overwrite the current
    /// models. If a client is not provided, a new one will be created using the
    /// OpenAI API key.
    pub async fn fetch_models(
        &mut self,
        client: Option<&Client>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.openai_api_key.is_empty() {
            return Err("OpenAI API key is empty. Can't fetch models.".into());
        }

        match client {
            Some(client) => {
                self.models = client.list_models().await?;
            }
            None => {
                self.models =
                    Client::new(&self.openai_api_key).list_models().await?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "gui")]
    pub fn ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if self.models.is_empty() {
            if ui.button("Fetch models").clicked() {
                // TODO: Somehow we need to send a message to our worker to
                // fetch the models and then get them back from a channel. This
                // is some work but we need to wrap the async stuff in it's own
                // thread because egui itself is not async. So we'll start an
                // executor in a worker and do like we do with `drama_llama`.
                // Alternatively we could just block the main thread and do it
                // on startup with futures::executor::block_on.

                // FIXME: This is blocking. We do have a way of sending a
                // command to the worker to fetch the models, but it's on the
                // parent struct, so we'll need to return some kind of command
                // from here to the parent to tell it to fetch the models. Then
                // when the models are ready, they're sent back to the main
                // thread and all is well with no blocking. But this is fine
                // for now.
                self.fetch_models_sync(None).ok();
            }
        } else {
            // We display a dropdown for the models and let the user select one.
            egui::ComboBox::from_label("Model")
                .selected_text(&self.chat_arguments.model)
                .show_ui(ui, |ui| {
                    for model in &self.models {
                        if ui
                            .selectable_label(
                                self.chat_arguments.model == model.id,
                                model.id.clone(),
                            )
                            .clicked()
                        {
                            self.chat_arguments.model = model.id.clone();
                        }
                    }
                });
        }

        // The OpenAI API key is a text field with password mode.
        ui.add(
            egui::TextEdit::singleline(&mut self.openai_api_key)
                .password(true)
                .hint_text("OpenAI API key"),
        );

        self.chat_arguments.ui(ui)
    }
}

// We're using the same interface as `drama_llama`. Eventually we can define a
// trait if all the stars align, but not so soon.
#[derive(Debug)]
pub(crate) enum Command {
    /// Worker should cancel any current generation, but not shut down. Dropping
    /// the channel will shut down the worker.
    Stop,
    /// Request models from the OpenAI API. The api key is required.
    FetchModels,
    /// Worker should start streaming predictions using the provided options.
    Predict { opts: ChatArguments },
}

#[derive(Debug)]
pub(crate) enum Response {
    /// Worker is done generating responses.
    Done,
    /// Models have been fetched and are available.
    Models {
        /// Available models. The UI should probably display these.
        models: Vec<openai_rust::models::Model>,
    },
    /// Worker is busy generating a response. Attached is the command that
    /// would have been acted upon.
    // although with OpenAI's streaming API and our design, there is no reason
    // we can't have concurrent generations going eventually, however there are
    // some changes that will have to be made in the App to handle this (since
    // we will have multiple heads). We will have to lock the UI as well to
    // prevent some cases like deleting a head while it's generating, however
    // starting new generations should be fine.
    // TODO: Handle the above carefully in the App. Try to break it.
    Busy { command: Command },
    /// The worker has predicted a piece of text along with OpenAI specific
    /// metadata
    // (since we're actually paying for it, might as well use it).
    // TODO: the `openai_rust` crate does not support logprobs, which I *do*
    // want to use eventually. I'll have to, add it to the crate, use `reqwest`
    // directly, or use another crate.
    Predicted { piece: String },
}

#[derive(Default)]
pub(crate) struct Worker {
    // We do need to run the executor in a separate thread. We can't run it in
    // the main thread because it's blocking.
    handle: Option<std::thread::JoinHandle<()>>,
    to_worker: Option<futures::channel::mpsc::Sender<Command>>,
    from_worker: Option<futures::channel::mpsc::Receiver<Response>>,
}

// we're going to use approximately the same API as `drama_llama` for now.
impl Worker {
    /// Start the worker thread. If the worker is already alive, this is a
    /// no-op. Use `restart` to restart the worker.
    pub fn start(&mut self, api_key: &str) {
        let api_key = api_key.to_string();
        if self.is_alive() {
            log::debug!("Worker is already alive");
            return;
        }

        let (to_worker, mut from_main) = futures::channel::mpsc::channel(128);
        // We get considerably more messages from the worker than we send to it,
        // and it's possible the UI might be blocked. For example, the ui does
        // not update unless it's interacted with and so the channel might fill
        // up, quite easily.
        // TODO: while generation is in progress, the ui should probably check
        // for this at regular intervals. The UI should likewise be redrawn
        // since otherwise you don't see the actual progress. Currently the
        // `try_recv` call in the main thread is only called when the user
        // interacts with the UI. The easiest temporary fix is to just call
        // every frame, and then we can optimize later. It's only downside is
        // CPU usage. There may be a regular interval function in egui that we
        // can use during generation.
        let (mut  to_main, from_worker) = futures::channel::mpsc::channel(4096);

        // Spawn the actual worker thread.
        let handle = std::thread::spawn(move || {
            use futures::{SinkExt, StreamExt};

            // We must use the tokio runtime since the `openai_rust` crate is
            // not reactor agnostic. This will be a problem for `wasm` use in
            // addition to the use of threads.
            let rt = tokio::runtime::Runtime::new().unwrap();
            let client = Client::new(&api_key);

            rt.block_on(async move {
                // The logic here is syncronous. We do want to wait for one
                // command to finish before starting the next one. Otherwise we
                // could use `for_each_concurrent` or something, but we would
                // have to associate the commands with the appropriate nodes.
                // This can wait until some changes in `App` and `Story` are
                // made so we can support multiple "heads" and lock the UI
                // appropriately.
                while let Some(command) = from_main.next().await {
                    let send_response = match command {
                        Command::Stop => {
                            // We are already stopped. We just tell main we're
                            // done.
                            to_main.send(Response::Done).await
                        }
                        Command::FetchModels => {
                            let models = match client.list_models().await {
                                Ok(models) => models,
                                Err(e) => {
                                    log::error!(
                                        "Couldn't fetch models: {}",
                                        e
                                    );
                                    // We can't send an error back to the main
                                    // thread yet. TODO: handle this and same
                                    // with `drama_llama`'s worker.
                                    return;
                                }
                            };

                            to_main
                                .send(Response::Models { models })
                                .await
                        }
                        Command::Predict { opts } => {
                            let args: openai_rust::chat::ChatArguments =
                                opts.into();
                            let mut stream =
                                match client.create_chat_stream(args).await {
                                    Ok(stream) => stream,
                                    Err(_) => todo!(),
                                };
                            
                            Ok('stream_loop: while let Some(Ok(mut chunk)) = stream.next().await {
                                // like with `drama_llama`, at this point we're
                                // going to check for stop signals. We could
                                // also `select!` on the channel and the stream
                                // to handle other commands concurrently, but
                                // I'm unsure about cancel safety at the moment.
                                // The docs on this in the openai crate are not
                                // specific on this. TODO: read source
                                while let Ok(cmd) = from_main.try_next() {
                                    match cmd {
                                        Some(Command::Stop) => {
                                            log::debug!("Generation cancelled.");
                                            // Break the outer loop which will
                                            // drop the stream and cancel the
                                            // generation. We will (hopefully)
                                            // not be billed for tokens we don't
                                            // use. The docs on whether this
                                            // will work are iffy since most are
                                            // written for Python, but it
                                            // *should* work.
                                            break 'stream_loop;
                                        }
                                        None => {
                                            // Main thread has dropped the
                                            // channel. This is our cue to exit.
                                            return;
                                        }
                                        Some(cmd) => {
                                            // We don't care about other
                                            // commands while generating. We
                                            // *could* handle them concurrently,
                                            // but not right now. For the moment
                                            // we will send them back as busy.
                                            to_main
                                                .send(Response::Busy { command: cmd })
                                                .await.ok();
                                        }
                                    }
                                }

                                // There is guaranteed to be at least one
                                // choice. We can't do anything with multiple
                                // yet.
                                let choice = &mut chunk.choices[0];

                                match choice.finish_reason.as_deref() {
                                    None => {   
                                        if let Some(delta) = choice.delta.content.take() {
                                            match to_main
                                                .send(Response::Predicted { piece: delta })
                                                .await {
                                                Ok(_) => {}
                                                Err(e) => {
                                                    log::error!(
                                                        "Couldn't send predicted piece: {}",
                                                        e
                                                    );
                                                    break 'stream_loop;
                                                }
                                            }
                                        }
                                    }
                                    Some("stop") => {
                                        to_main.send(Response::Done).await;
                                        break 'stream_loop;
                                    }
                                    Some("max_tokens") => {
                                        to_main.send(Response::Done).await;
                                        break 'stream_loop;
                                    }

                                    Some(reason) => {
                                        log::error!("Unknown finish reason: {reason:?}");
                                        to_main.send(Response::Done).await;
                                        break 'stream_loop;
                                    }
                                }
                            })
                        }
                    };

                    match send_response {
                        Ok(_) => {
                            // Response sent successfully. We can now accept the
                            // next command.
                        }
                        Err(e) => {
                            if e.is_disconnected() {
                                // Main thread has dropped the receiving channel
                                // so we can exit.
                                return;
                            } else {
                                // The channel is full. This is bad. We should
                                // exit rather than waste tokens.
                                log::error!("Couldn't send response: {}", e);
                                return;
                            }
                        }
                    }
                }
            });
        });

        self.handle = Some(handle);
        self.to_worker = Some(to_worker);
        self.from_worker = Some(from_worker);
    }

    /// Stop current generation after the next token. Does not shut down the
    /// worker thread. Does not block. Does not guarantee that generation will
    /// stop immediately. Use `shutdown` to shut down the worker.
    /// 
    /// If the channel is full, or if the worker is not alive, this will return
    /// an error. In this case await `stop` instead or terminate the process,
    /// since it shouldn't happen. If the channel is full the UI is flooding the
    /// channel with requests which shouldn't happen since the worker checks for
    /// commands at regular intervals, sending them back as `Busy` if it's
    /// currently generating.
    pub fn try_stop(&mut self) -> Result<(), futures::channel::mpsc::TrySendError<Command>> {
        log::debug!("Telling worker to cancel current generation.");
        if let Some(to_worker) = self.to_worker.as_mut() {
            to_worker.try_send(Command::Stop)?;
        }

        Ok(())
    }

    /// Same as try_stop, but awaits the result.
    pub async fn stop(&mut self) -> Result<(), futures::channel::mpsc::SendError> {
        log::debug!("Waiting for worker to cancel current generation.");
        if let Some(to_worker) = self.to_worker.as_mut() {
            to_worker.send(Command::Stop).await?;
        }

        Ok(())
    }

    /// Shutdown the worker thread. If the worker is not alive, this is a no-op.
    /// 
    /// This will block until the worker is done (the next piece is yielded) if
    /// generation is in progress. Otherwise it will return (almost)
    /// immediately.
    /// 
    /// This can only return an error in the case where the worker thread's
    /// receiver is full. This should not happen. If it does, the UI is sending
    /// too many requests. This is a bug in the UI code and/or the worker since
    /// this shouldn't be possible.
    pub fn shutdown(&mut self) -> Result<(), futures::channel::mpsc::TrySendError<Command>> {
        match self.try_stop() {
            Ok(_) => {
                // we sent the stop command. Now we can drop the channel to
                // trigger the worker to shut down.
            },
            Err(e) => {
                if e.is_disconnected() {
                    // The worker is already shut down. We can just return.
                    return Ok(());
                } else {
                    // The channel is full. This is bad.
                    return Err(e);
                }
            },
        }
        log::debug!("Telling worker to shut down.");
        if let Some(mut to_worker) = self.to_worker.take() {
            // I'm unsure of the order of these. I think we should close first
            // and then flush. I'm not sure if we need to do both.
            // TODO: test this.
            to_worker.close();
            to_worker.flush();
            // worker dropped, the worker thread should terminate next iteration
        }

        if let Some(_) = self.from_worker.take() {
            // drop receiver. This will cause an error with any sends in
            // progress which will terminate the worker thread if it's still
            // alive.
        }

        // finally, we wait for the worker thread to finish.
        if let Some(handle) = self.handle.take() {
            handle.join().ok();
        }

        Ok(())
    }

    /// Returns true if the worker thread is alive.
    pub fn is_alive(&self) -> bool {
        self.handle.is_some()
    }

    /// Start prediction. Returns any SendError that occurs. This does not block
    /// the current thread. Use `shutdown` to stop the worker thread.
    /// 
    /// # Panics
    /// * If the worker is not alive.
    pub fn predict(&mut self, opts: ChatArguments) -> Result<(), futures::channel::mpsc::TrySendError<Command>> {
        if !self.is_alive() {
            // So the futures API does not allow us to construct an error since
            // the fields are private and the only constructors are private.
            // Do we panic? this does indicate a logic error in the code. Yes,
            // since I don't feel like writing a custom error type for this.
            panic!("Worker is not alive. Can't predict.");
        }

        if let Some(to_worker) = self.to_worker.as_mut() {
            to_worker.try_send(Command::Predict { opts })?;
        }

        Ok(())
    }

    /// Try to receive a response from the worker. This does not block. If the
    /// worker is not alive, this will return None. If the channel is empty or
    /// closed, Some(error) is returned.
    pub fn try_recv(&mut self) -> Option<Result<Response, futures::channel::mpsc::TryRecvError>> {
        if let Some(from_worker) = self.from_worker.as_mut() {
            match from_worker.try_next() {
                // channel has a response
                Ok(Some(response)) => Some(Ok(response)),
                // channel is closed and no more messages in the queue
                Ok(None) => {
                    // There shouldn't happen, but if it does we should clean
                    // up the worker.
                    self.shutdown().ok();
                    None
                },
                Err(e) => Some(Err(e)),
            }
        } else {
            None
        }
    }
}
