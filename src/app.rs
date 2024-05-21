mod settings;

use {self::settings::Settings, crate::story::Story};

#[derive(Default, PartialEq, derive_more::Display)]
pub enum SidebarPage {
    #[default]
    Stories,
    Settings,
}

#[derive(Default)]
pub struct Sidebar {
    // New story title buffer
    title_buf: String,
    page: SidebarPage,
}
#[derive(Default)]
pub struct App {
    active_story: Option<usize>,
    stories: Vec<Story>,
    settings: Settings,
    sidebar: Sidebar,
    /// Modal error message text. If this is `Some`, the UI should display an
    /// error message.
    errmsg: Option<String>,
    #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
    drama_llama_worker: crate::drama_llama::Worker,
    #[cfg(feature = "openai")]
    openai_worker: crate::openai::Worker,
    #[cfg(feature = "generate")]
    generation_in_progress: bool,
    #[cfg(not(target_arch = "wasm32"))]
    save_dialog: Option<egui_file::FileDialog>,
    #[cfg(not(target_arch = "wasm32"))]
    saving_txt: bool,
}

// {"default_author":"","prompt_include_authors":false,"prompt_include_title":false,"selected_generative_backend":"OpenAI","backend_options":{"DramaLlama":{"DramaLlama":{"model":"","predict_options":{"n":512,"seed":1337,"stop_sequences":[],"stop_strings":[],"regex_stop_sequences":[],"sample_options":{"modes":[],"repetition":null}}}},"OpenAI":{"OpenAI":{"settings":{"openai_api_key":"hidden in keyring","chat_arguments":{"model":"gpt-3.5-turbo","messages":[{"role":"system","content":"A user and an assistant are collaborating on a story. The user starts by writing a paragraph, then the assistant writes a paragraph, and so on. Both will be credited for the end result.'"},{"role":"user","content":"Hi, GPT! Let's write a story together."},{"role":"assistant","content":"Sure, I'd love to help. How about you start us off? I'll try to match your tone and style."}],"temperature":1.0,"top_p":1.0,"n":null,"stop":null,"max_tokens":1024,"presence_penalty":0.0,"frequency_penalty":0.0,"user":null}}}}}}

impl App {
    pub fn new<'s>(cc: &eframe::CreationContext<'s>) -> Self {
        let ctx = cc.egui_ctx.clone();

        let stories = cc
            .storage
            .map(|storage| {
                storage
                    .get_string("stories")
                    .and_then(|s| {
                        log::debug!("Loading stories: {}", s);
                        match serde_json::from_str(&s) {
                            Ok(stories) => Some(stories),
                            Err(e) => {
                                log::error!("Failed to load stories: {}", e);
                                None
                            }
                        }
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let settings = cc
            .storage
            .map(|storage| {
                storage
                    .get_string("settings")
                    .and_then(|s| {
                        log::debug!("Loading settings: {}", s);
                        match serde_json::from_str(&s) {
                            Ok(settings) => Some(settings),
                            Err(e) => {
                                log::error!("Failed to load settings: {}", e);
                                None
                            }
                        }
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        #[allow(unused_mut)]
        let mut new = Self {
            stories,
            settings,
            active_story: None,
            ..Default::default()
        };

        // Handle generation backends
        if let Err(e) = new.start_generative_backend(ctx) {
            eprintln!("Failed to start generative backend: {}", e);
            // This is fine. It can be restarted later once settings are fixed
            // or the user chooses a different backend.
        }

        new
    }

    pub fn new_story(&mut self, title: String, author: String) {
        self.stories.push(Story::new(title, author));
        self.active_story = Some(self.stories.len() - 1);
    }

    /// (active) story
    pub fn story(&self) -> Option<&Story> {
        self.active_story.map(|i| &self.stories[i])
    }

    /// (active) story
    pub fn story_mut(&mut self) -> Option<&mut Story> {
        self.active_story.map(move |i| self.stories.get_mut(i))?
    }

    /// Starts the generative backend if it is not already running. A context
    /// is required to request redraws from the worker thread.
    #[cfg(feature = "generate")]
    pub fn start_generative_backend(
        &mut self,
        context: egui::Context,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!(
            "Starting generative backend: {}",
            self.settings.selected_generative_backend
        );
        self.settings.setup();

        match self.settings.backend_options() {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            settings::BackendOptions::DramaLlama { model, .. } => {
                self.drama_llama_worker.start(model.clone(), context)?;
            }
            #[cfg(feature = "openai")]
            settings::BackendOptions::OpenAI { settings } => {
                self.openai_worker.start(&settings.openai_api_key, context);
            }
        }

        Ok(())
    }

    /// Reset the generative backend to the default. This should initialize or
    /// restart the backend.
    #[cfg(feature = "generate")]
    pub fn reset_generative_backend(
        &mut self,
        context: egui::Context,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.shutdown_generative_backend()?;
        self.start_generative_backend(context)?;

        Ok(())
    }

    /// Start generation (with current settings, at the story head).
    // TODO: Move backend code to the backend modules. This function is too
    // long. Each backend does more or less the same thing. See if we can make
    // a trait for this.
    #[cfg(feature = "generate")]
    pub fn start_generation(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.generation_in_progress {
            // If this happens, some UI element is not locked properly.
            panic!("Generation already in progress. This is a bug. Please report it.");
        }

        #[cfg(all(feature = "generate", not(target_arch = "wasm32")))]
        {
            let include_authors = self.settings.prompt_include_authors;
            let include_title = self.settings.prompt_include_title;
            let backend_options = self.settings.backend_options();
            let model_name = backend_options.model_name().to_string();

            match backend_options {
                #[cfg(all(
                    feature = "drama_llama",
                    not(target_arch = "wasm32")
                ))]
                settings::BackendOptions::DramaLlama {
                    predict_options,
                    ..
                } => {
                    let predict_options = predict_options.clone();

                    // This has to go here because this and `backend_options`
                    // are mutably borrowed. We don't use `backend_options`
                    // after this, so it's fine.
                    let story = if let Some(story) = self.story_mut() {
                        story.add_author(model_name);
                        story
                    } else {
                        // This should not happen.
                        panic!("Generation request without active story. Please report this. This is a bug.");
                    };

                    // Format the story for generation. In the case of
                    // LLaMA, it's raw text. We're expecting a foundation
                    // model, rather than a chat or instruct model. Those
                    // may work, but are not officially supported by Weave.
                    let mut text = String::new();
                    story
                        .format_full(&mut text, include_authors, include_title)
                        .unwrap();

                    match self
                        .drama_llama_worker
                        // We do want to clone the options because they can be
                        // changed during generation.
                        .predict(text, predict_options.clone())
                    {
                        Ok(_) => {
                            // This flag is used to lock the UI while generation
                            // is in progress.
                            self.generation_in_progress = true;
                        }
                        Err(e) => {
                            self.generation_in_progress = false;
                            return Err(e.into());
                        }
                    }
                }
                #[cfg(feature = "openai")]
                settings::BackendOptions::OpenAI { settings } => {
                    let mut options = settings.chat_arguments.clone();

                    let story = if let Some(story) = self.story_mut() {
                        story.add_author(model_name);
                        story
                    } else {
                        // This should not happen.
                        panic!("Generation request without active story. Please report this. This is a bug.");
                    };

                    // append the story to the system prompt and intro messages.
                    // The last message will always be `user` since we're
                    // expecting a response from `assistant` and we specified in
                    // the default system prompt that the turns will alternate.
                    // TODO: Keep track of authors of each node and only allow
                    // generation from a user's node... maybe.
                    options.messages.extend(story.to_openai_messages());

                    match self.openai_worker.predict(options) {
                        Ok(_) => {
                            self.generation_in_progress = true;
                        }
                        Err(e) => {
                            if e.is_disconnected() {
                                // This can happen for a variety of reasons,
                                // like the connection failing or some other
                                // error like a bad API key. No matter what, we
                                // should unlock the UI so the worker can be
                                // restarted.
                                self.generation_in_progress = false;
                            } else {
                                // Channel is full. This is bad.
                                panic!("OpenAI worker command channel is full. This is a bug. Please report this: {}", e)
                            }
                            return Err(e.into());
                        }
                    }
                }
            }

            Ok(())
        }
    }

    /// Stop generation.
    #[cfg(feature = "generate")]
    pub fn stop_generation(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.settings.selected_generative_backend {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            settings::GenerativeBackend::DramaLlama => {
                self.drama_llama_worker.stop()?;
            }
            #[cfg(feature = "openai")]
            settings::GenerativeBackend::OpenAI => {
                self.openai_worker.try_stop()?;
            }
        }

        Ok(())
    }

    /// Stop generation. Shutdown the generative backend. This may block until
    /// the next piece is yielded.
    #[cfg(feature = "generate")]
    pub fn shutdown_generative_backend(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.settings.selected_generative_backend {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            settings::GenerativeBackend::DramaLlama => {
                if self.drama_llama_worker.shutdown().is_err() {
                    return Err("`drama_llama` worker thread did not shut down cleanly.".into());
                }
            }
            #[cfg(feature = "openai")]
            settings::GenerativeBackend::OpenAI => {
                self.openai_worker.shutdown()?;
            }
        }

        Ok(())
    }

    /// Draw sidebar.
    pub fn draw_sidebar(
        &mut self,
        ctx: &eframe::egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::SidePanel::left("sidebar")
            .default_width(200.0)
            .resizable(true)
            .show(ctx, |ui| {
                // Stuff could break if the user changes the story or backend
                // settings while generation is in progress. The easiest way to
                // fix this is just to make such actions impossible so we'll
                // replace the sidebar with generation controls.
                #[cfg(feature = "generate")]
                if self.generation_in_progress {
                    ui.heading("Generating...");
                    if ui.button("Stop").clicked() {
                        #[cfg(all(
                            feature = "drama_llama",
                            not(target_arch = "wasm32")
                        ))]
                        {
                            // This requests a stop, so we don't change the flag
                            // here, rather when the backend responds.
                            if let Err(e) = self.drama_llama_worker.stop() {
                                // Most likely worker is dead
                                eprintln!(
                                    "Failed to stop drama llama worker: {}",
                                    e
                                );
                            }
                        }
                    }
                    // Return early so we don't draw the rest of the sidebar.
                    return;
                }

                // These are our sidebar tabs.
                // TODO: better tabs and layout
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.sidebar.page,
                        SidebarPage::Stories,
                        "Stories",
                    );
                    ui.selectable_value(
                        &mut self.sidebar.page,
                        SidebarPage::Settings,
                        "Settings",
                    );
                });

                ui.heading(self.sidebar.page.to_string());

                match self.sidebar.page {
                    SidebarPage::Settings => {
                        if let Some(action) = self.settings.draw(ui) {
                            self.handle_settings_action(action, ctx);
                        }
                    }
                    SidebarPage::Stories => {
                        self.draw_stories_tab(ui);
                    }
                }
            });
    }

    /// Draw error message if there is one. Returns `true` if the error message
    /// is displayed. This function accepts a closure which can be used to
    /// display additional UI elements, such as a button to handle the error.
    pub fn draw_error_message(
        &mut self,
        ctx: &egui::Context,
        mut f: Option<Box<dyn FnMut(&mut egui::Ui)>>,
    ) -> bool {
        let mut closed = false; // because two mutable references
        if let Some(msg) = &self.errmsg {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::Window::new("Error").show(ui.ctx(), |ui| {
                    ui.label(msg);
                    ui.horizontal(|ui| {
                        if ui.button("Close").clicked() {
                            closed = true;
                        }
                        if let Some(f) = &mut f {
                            f(ui);
                        }
                    })
                });
            });
        } else {
            return false;
        }
        if closed {
            self.errmsg = None;
            return false;
        } else {
            return true;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn draw_save_buttons(&mut self, ui: &mut egui::Ui) {
        ui.label("Save");
        ui.horizontal(|ui| {
            let filter = Box::new(move |path: &std::path::Path| {
                path.extension().map_or(false, |ext| ext == "json")
            });

            let save_btn = ui
                .button("Save")
                .on_hover_text_at_pointer("Save story to JSON.");

            let export = ui
                .button("Export")
                .on_hover_text_at_pointer("Export active story path to Markdown.");

            let load_btn = ui
                .button("Load")
                .on_hover_text_at_pointer("Load story from JSON.");

            if save_btn.clicked() {
                let mut dialog = egui_file::FileDialog::save_file(None)
                        .show_files_filter(filter);
                dialog.open();

                self.save_dialog = Some(dialog);
            } else if load_btn.clicked() {
                let mut dialog = egui_file::FileDialog::open_file(None)
                        .show_files_filter(filter);
                dialog.open();

                self.saving_txt = false;
                self.save_dialog = Some(dialog);
            } else if export.clicked() {
                let filter = Box::new(move |path: &std::path::Path| {
                    path.extension().map_or(false, |ext| ext == "md")
                });

                let mut dialog = egui_file::FileDialog::open_file(None)
                        .show_files_filter(filter);
                dialog.open();

                self.saving_txt = true;
                self.save_dialog = Some(dialog);
            }

            if let Some(dialog) = &mut self.save_dialog {
                if dialog.show(ui.ctx()).selected() {
                    if let Some(path) = dialog.path() {
                        match dialog.dialog_type() {
                            egui_file::DialogType::OpenFile => {
                                let text = match std::fs::read_to_string(path) {
                                    Ok(text) => text,
                                    Err(e) => {
                                        self.errmsg = Some(format!(
                                            "Failed to read `{:?}` because: {}",
                                            path,
                                            e
                                        ));
                                        return;
                                    }
                                };
                                let story:Story = match serde_json::from_str(&text) {
                                    Ok(story) => story,
                                    Err(e) => {
                                        self.errmsg = Some(format!(
                                            "Failed to parse `{:?}` because: {}",
                                            path,
                                            e
                                        ));
                                        return;
                                    }
                                };

                                self.stories.push(story);
                            },
                            egui_file::DialogType::SaveFile => {
                                let active_story_index = match self.active_story {
                                    Some(i) => i,
                                    None => {
                                        self.errmsg = Some("No active story to save.".to_string());
                                        return;
                                    }
                                };

                                let payload = if self.saving_txt {
                                    self.stories[active_story_index].to_string()
                                } else {
                                    match serde_json::to_string(&self.stories[active_story_index]) {
                                        Ok(json) => json,
                                        Err(e) => {
                                            self.errmsg = Some(format!(
                                                "Failed to serialize stories because: {}",
                                                e
                                            ));
                                            return;
                                        }
                                    }
                                };

                                match std::fs::write(path, payload) {
                                    Ok(_) => {},
                                    Err(e) => {
                                        self.errmsg = Some(format!(
                                            "Failed to write `{:?}` because: {}",
                                            path,
                                            e
                                        ));
                                        return;
                                    }
                                }
                            },
                            egui_file::DialogType::SelectFolder => {
                                unreachable!("Because we don't instantiate this type above.")
                            },
                        }
                    }
                    self.save_dialog = None;
                }
            }
        });
    }

    /// Handle settings action.
    pub fn handle_settings_action(
        &mut self,
        action: settings::Action,
        context: &egui::Context,
    ) {
        match action {
            settings::Action::SwitchBackends { from, to } => {
                debug_assert!(from != to);
                debug_assert!(
                    self.settings.selected_generative_backend == from
                );

                if let Err(e) = self.stop_generation() {
                    eprintln!("Failed to stop generation: {}", e);
                }

                self.settings.selected_generative_backend = to;

                if let Err(e) = self.reset_generative_backend(context.clone()) {
                    eprintln!("Failed to start generative backend: {}", e);
                }

                self.settings.pending_backend_switch = None;
            }
        }
    }

    /// Draw the stories sidebar tab.
    fn draw_stories_tab(&mut self, ui: &mut egui::Ui) {
        let mut delete = None;
        for (i, story) in self.stories.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button("X").clicked() {
                    delete = Some(i);
                }
                if ui.button(&story.title).clicked() {
                    self.active_story = Some(i);
                }
            });
        }
        if let Some(i) = delete {
            self.stories.remove(i);
            if self.active_story == Some(i) {
                self.active_story = None;
            }
        }

        ui.horizontal(|ui| {
            if ui.button("New").clicked() {
                let title = self.sidebar.title_buf.clone();
                let author = self.settings.default_author.clone();
                self.new_story(title, author);
                self.sidebar.title_buf.clear();
            }
            ui.text_edit_singleline(&mut self.sidebar.title_buf);
        });

        // We might not support wasm at all, but if we do this will have to be
        // implemented differently. Skip it for now.
        #[cfg(not(target_arch = "wasm32"))]
        self.draw_save_buttons(ui);
    }

    /// Draw the central panel.
    pub fn draw_central_panel(
        &mut self,
        ctx: &eframe::egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        // Because mutable references, we need to copy these flags.
        let mut start_generation = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut new_pieces = Vec::new();

            self.update_generation(&mut new_pieces);

            // TODO: make it possible to scroll the node view. The nodes are
            // currently windows which cannot be in a scroll area. They float.
            // It would have been nice to know this before, but oh well. One
            // solution suggested in the following issue is to use an area
            // within an area:
            // https://github.com/emilk/egui/discussions/3290
            // Another is to make a custom widget. Either is a bunch of work,
            // but the latter might be more flexible. `Window` also does a lot
            // we don't actually need.
            // Probably less work is actually use `wgpu` to render the nodes in
            // the viewport. It's less work than it sounds, and probably less
            // than the other solutions which might integrate better with egui,
            // but might be more work to implement and maintain. A `wgpu`
            // solution might perform better as well and I have some experience
            // with it.
            // In the meantime, the windows are, at least, collapsible.
            let generation_in_progress = self.generation_in_progress;
            if let Some(story) = self.story_mut() {
                // TODO: the response from story.draw could be more succinct. We
                // only realy know if we need to start generation (for now).
                if let Some(action) = story.draw(ui, generation_in_progress) {
                    if action.continue_ | action.generate.is_some() {
                        // The path has already been changed. We need only
                        // start generation.
                        start_generation = true;
                    }
                }
                story.extend_paragraph(new_pieces);
            } else {
                if !new_pieces.is_empty() {
                    // We received a piece of text but there is no active story.
                    // This should not happen.
                    eprintln!(
                        "Received pieces but no active story: {new_pieces:?}"
                    );
                }
                ui.heading("Welcome to Weave!");
                ui.label("Create a new story or select an existing one.");
            }
        });

        if start_generation {
            if let Err(e) = self.start_generation() {
                log::error!("Failed to start generation: {}", e);
            }
        }
    }

    /// Update `new_pieces` with any newly generated pieces of text.
    #[cfg(feature = "generate")]
    fn update_generation(&mut self, new_pieces: &mut Vec<String>) {
        use settings::GenerativeBackend;

        if !self.generation_in_progress {
            return;
        }

        match self.settings.selected_generative_backend {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            GenerativeBackend::DramaLlama => {
                // Handle responses from the drama llama worker.
                match self.drama_llama_worker.try_recv() {
                    Some(Err(e)) => match e {
                        std::sync::mpsc::TryRecvError::Empty => {
                            // The channel is empty. This is normal.
                        }
                        std::sync::mpsc::TryRecvError::Disconnected => {
                            eprintln!(
                            "`drama_llama` worker disconnected unexpectedly."
                        );
                            // This should not happen, but it can if the worker
                            // panics. This indicates a bug in `drama_llama`.
                            if let Err(err) = self.drama_llama_worker.shutdown()
                            {
                                eprintln!(
                                    "Worker thread died because: {:?}",
                                    err
                                );
                            }
                            self.generation_in_progress = false;
                        }
                    },
                    Some(Ok(response)) => match response {
                        // The worker has generated a new piece of text, we add
                        // it to the story.
                        crate::drama_llama::Response::Predicted { piece } => {
                            new_pieces.push(piece);
                        }
                        crate::drama_llama::Response::Done => {
                            // Trim whitespace from the end of the story. The
                            // Predictor currently keeps any end sequence, which
                            // might be whitespace.
                            // TODO: add a setting to control this behavior in
                            // `drama_llama`
                            if let Some(story) = self.story_mut() {
                                story.head_mut().trim_end_whitespace();
                            }
                            // We can unlock the UI now.
                            self.generation_in_progress = false;
                        }
                        crate::drama_llama::Response::Busy { request } => {
                            // This might happen because of data races, but really
                            // shouldn't.
                            // TODO: gui error message
                            log::error!(
                                "Unexpected request sent to worker. Report this please: {:?}",
                                request
                            )
                        }
                    },
                    None => {
                        // Worker is dead.
                        self.generation_in_progress = false;
                    }
                }
            }
            #[cfg(feature = "openai")]
            GenerativeBackend::OpenAI => match self.openai_worker.try_recv() {
                Some(Err(_)) => {
                    // In this case the worker isn't dead. This is the normal
                    // case when the channel is empty, but still connected. The
                    // api for this channel is not the same as for
                    // std::sync::mpsc
                }
                Some(Ok(response)) => match response {
                    crate::openai::Response::Predicted { piece } => {
                        new_pieces.push(piece);
                    }
                    crate::openai::Response::Done => {
                        if let Some(story) = self.story_mut() {
                            story.head_mut().trim_end_whitespace();
                        }
                        self.generation_in_progress = false;
                    }
                    crate::openai::Response::Busy { request } => {
                        log::error!(
                                "Unexpected request sent to worker. Report this please: {:?}",
                                request
                            )
                    }
                    crate::openai::Response::Models { models } => {
                        // because conditional compilation
                        #[allow(irrefutable_let_patterns)]
                        if let settings::BackendOptions::OpenAI { settings } =
                            self.settings.backend_options()
                        {
                            settings.models = models;
                        }
                    }
                },
                None => {
                    // Worker is dead.
                    self.generation_in_progress = false;
                }
            },
            #[allow(unreachable_patterns)] // because conditional compilation
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn update(
        &mut self,
        ctx: &eframe::egui::Context,
        frame: &mut eframe::Frame,
    ) {
        if self.draw_error_message(ctx, None) {
            // An error message is displayed. We skip the rest of the UI.
            return;
        }
        self.draw_sidebar(ctx, frame);
        self.draw_central_panel(ctx, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let serialized_stories = serde_json::to_string(&self.stories).unwrap();
        let serialized_settings =
            serde_json::to_string(&self.settings).unwrap();

        log::debug!("Saving stories: {}", serialized_stories);
        log::debug!("Saving settings: {}", serialized_settings);

        storage.set_string("stories", serialized_stories);
        storage.set_string("settings", serialized_settings);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = self.shutdown_generative_backend() {
            eprintln!("Failed to cleanly shut down generative backend: {}", e);
        }
    }
}
