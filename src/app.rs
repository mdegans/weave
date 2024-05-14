use crate::{settings::Settings, story::Story};

#[derive(Default)]
pub struct Toolbar {
    pub title_buf: String,
}

pub struct Viewport {
    pub scroll: egui::Vec2,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scroll: Default::default(),
            zoom: 1.0,
        }
    }
}

#[derive(Default, derive_more::Display)]
pub enum SidebarPage {
    #[default]
    Stories,
    Settings,
}

#[derive(Default)]
pub struct Sidebar {
    page: SidebarPage,
}
#[derive(Default)]
pub struct App {
    active_story: Option<usize>,
    stories: Vec<Story>,
    settings: Settings,
    sidebar: Sidebar,
    toolbar: Toolbar,
    #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
    drama_llama_worker: crate::drama_llama::Worker,
    #[cfg(feature = "generate")]
    generation_in_progress: bool,
}

impl App {
    pub fn new<'s>(cc: &eframe::CreationContext<'s>) -> Self {
        let stories = cc
            .storage
            .map(|storage| {
                storage
                    .get_string("stories")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let settings = cc
            .storage
            .map(|storage| {
                storage
                    .get_string("settings")
                    .and_then(|s| serde_json::from_str(&s).ok())
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
        new.start_generative_backend();

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

    /// Starts the generative backend if it is not already running.
    #[cfg(feature = "generate")]
    pub fn start_generative_backend(&mut self) {
        log::info!(
            "Starting generative backend: {}",
            self.settings.selected_generative_backend
        );
        #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
        {
            if matches!(
                self.settings.selected_generative_backend,
                crate::settings::GenerativeBackend::DramaLlama
            ) {
                // Apply any model specific settings, for example, context size
                // which might be shorter than the context length of the
                // predict options. It won't cause a crash, but it will cause
                // unexpected behavior. This also makes sure any UI widgets are
                // clamped to valid values.
                self.settings.configure_for_current_local_model();
                let options = self.settings.backend_options();
                match self.drama_llama_worker.start(options.into()) {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!(
                            "Failed to start `drama_llama` worker: {e}"
                        );
                    }
                }
            }
        }
    }

    /// Reset the generative backend to the default. This should initialize or
    /// restart the backend.
    #[cfg(feature = "generate")]
    pub fn reset_generative_backend(&mut self) {
        #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
        {
            if matches!(
                self.settings.selected_generative_backend,
                crate::settings::GenerativeBackend::DramaLlama
            ) {
                let options = self.settings.backend_options();
                match self.drama_llama_worker.restart(options.into()) {
                    Ok(_) => {}
                    Err(e) => {
                        // TODO: gui error message (use channel?)
                        eprintln!("Failed to start drama llama worker: {}", e);
                    }
                }
            }
        }
    }

    /// Stop generation.
    #[cfg(feature = "generate")]
    pub fn stop_generation(&mut self) {
        #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
        {
            if let Err(e) = self.drama_llama_worker.stop() {
                // Most likely worker is dead
                eprintln!(
                    "Could not stop `drama_llama` cleanly because: {e:#?}",
                );
            }
        }
    }

    /// Shutdown the generative backend. This is a no-op for most backends.
    #[cfg(feature = "generate")]
    pub fn shutdown_generative_backend(&mut self) {
        #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
        {
            if let Err(e) = self.drama_llama_worker.shutdown() {
                // Most likely worker is dead
                eprintln!("`drama_llama` worker did not shut down cleanly because: {e:#?}");
            }
        }
    }

    /// Draw the toolbar.
    pub fn draw_toolbar(
        &mut self,
        ctx: &eframe::egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::TopBottomPanel::top("toolbar")
            .resizable(true)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    if ui.button("New Story").clicked() {
                        let title = self.toolbar.title_buf.clone();
                        let author = self.settings.default_author.clone();
                        self.new_story(title, author);
                        self.toolbar.title_buf.clear();
                    }
                    ui.text_edit_singleline(&mut self.toolbar.title_buf);
                });
            });
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
                    if ui.button("Stories").clicked() {
                        self.sidebar.page = SidebarPage::Stories;
                    }
                    if ui.button("Settings").clicked() {
                        self.sidebar.page = SidebarPage::Settings;
                    }
                });

                ui.heading(self.sidebar.page.to_string());

                match self.sidebar.page {
                    SidebarPage::Settings => {
                        self.settings.draw(ui);
                    }
                    SidebarPage::Stories => {
                        self.draw_stories_tab(ui);
                    }
                }
            });
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

        #[cfg(feature = "generate")]
        {
            // FIXME: The generate button should go on nodes, not here.
            if !self.stories.is_empty() && ui.button("Generate").clicked() {
                self.start_generative_backend();
                self.generation_in_progress = true;
                match self.settings.selected_generative_backend {
                    #[cfg(all(
                        feature = "drama_llama",
                        not(target_arch = "wasm32")
                    ))]
                    crate::settings::GenerativeBackend::DramaLlama => {
                        let options = self.settings.backend_options();
                        let predict_options = options.into();
                        let model: String = options.model_name().to_string();
                        let include_authors =
                            self.settings.prompt_include_authors;
                        let include_title = self.settings.prompt_include_title;

                        if let Some(story) = self.story_mut() {
                            story.add_author(model.clone());
                            let empty: Vec<String> = Vec::new();
                            story.add_paragraph(model, &empty);
                            let mut text = String::new();
                            story
                                .format_full(
                                    &mut text,
                                    include_authors,
                                    include_title,
                                )
                                .unwrap();
                            log::debug!("Prompt: ```{text}```");
                            match self
                                .drama_llama_worker
                                .predict(text, predict_options)
                            {
                                Ok(_) => {
                                    // Text submitted to worker.
                                }
                                Err(e) => {
                                    self.generation_in_progress = false;
                                    eprint!(
                                        "Failed to send command to worker: {e}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Draw the central panel.
    pub fn draw_central_panel(
        &mut self,
        ctx: &eframe::egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut new_pieces = Vec::new();

            self.update_generation(&mut new_pieces);

            if let Some(story) = self.story_mut() {
                story.draw(ui);
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
    }

    /// Update any generation that is in progress.
    #[cfg(feature = "generate")]
    fn update_generation(&mut self, new_pieces: &mut Vec<String>) {
        use crate::settings::GenerativeBackend;

        if !self.generation_in_progress {
            return;
        }

        #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
        if matches!(
            self.settings.selected_generative_backend,
            GenerativeBackend::DramaLlama
        ) {
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
                        if let Err(err) = self.drama_llama_worker.shutdown() {
                            eprintln!("Worker thread died because: {:?}", err);
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
                        self.generation_in_progress = false;
                        // TODO: strip any trailing whitespace. This will
                        // require some changes to `Node` so that a valid piece
                        // is still at the end. (we can't just trim the text).
                    }
                    crate::drama_llama::Response::Busy { command } => {
                        // This might happen because of data races.
                        // TODO: gui error message
                        todo!("gui error message: {:?}", command)
                    }
                },
                None => {
                    // Worker is dead.
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(
        &mut self,
        ctx: &eframe::egui::Context,
        frame: &mut eframe::Frame,
    ) {
        self.draw_toolbar(ctx, frame);
        self.draw_sidebar(ctx, frame);
        self.draw_central_panel(ctx, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            "stories",
            serde_json::to_string(&self.stories).unwrap(),
        );

        storage.set_string(
            "settings",
            serde_json::to_string(&self.settings).unwrap(),
        );
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_generative_backend();
    }
}
