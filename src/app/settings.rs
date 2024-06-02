use serde::{Deserialize, Serialize};

/// Backend for generation.
#[cfg(feature = "generate")]
#[derive(
    Clone,
    Copy,
    Debug,
    derive_more::Display,
    Deserialize,
    Eq,
    Hash,
    PartialEq,
    Serialize,
)]
pub enum GenerativeBackend {
    #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
    DramaLlama,
    #[cfg(feature = "ollama")]
    Ollama,
    #[cfg(feature = "openai")]
    OpenAI,
    #[cfg(feature = "claude")]
    Claude,
}

#[cfg(feature = "generate")]
impl GenerativeBackend {
    /// All the generative backends that can be used, in order of preference.
    pub const ALL: &'static [&'static GenerativeBackend] = &[
        #[cfg(feature = "drama_llama")]
        &GenerativeBackend::DramaLlama,
        #[cfg(feature = "ollama")]
        &GenerativeBackend::Ollama,
        #[cfg(feature = "openai")]
        &GenerativeBackend::OpenAI,
        #[cfg(feature = "claude")]
        &GenerativeBackend::Claude,
    ];

    pub const DEFAULT: &'static GenerativeBackend = if Self::ALL.is_empty() {
        panic!(
            "There must be at least one generative backend feature enabled to use the `generate` feature."
        );
    } else {
        Self::ALL[0]
    };

    pub fn supports_model_view(&self) -> bool {
        match self {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            GenerativeBackend::DramaLlama => true,
            // We don't actually know how the OpenAI model is prompted since we
            // feed it messages, not raw text. We could make a good educated
            // guess, but it's not worth it right now.
            #[cfg(feature = "openai")]
            GenerativeBackend::OpenAI => false,
        }
    }
}

#[cfg(feature = "generate")]
impl Default for GenerativeBackend {
    fn default() -> Self {
        *Self::DEFAULT
    }
}

#[cfg(feature = "generate")]
#[derive(Serialize, Deserialize)]
pub enum BackendOptions {
    #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
    DramaLlama {
        #[serde(default)]
        model: std::path::PathBuf,
        #[serde(default)]
        predict_options: drama_llama::PredictOptions,
        #[serde(skip)]
        // This has to go here because of mutable references and lifetimes.
        file_dialog: Option<egui_file::FileDialog>,
        #[serde(skip)]
        // Maximum context size for the model. This is set when the model is
        // loaded and is used to clamp the context size in the UI.
        max_context_size: usize,
    },
    #[cfg(feature = "ollama")]
    Ollama,
    #[cfg(feature = "openai")]
    OpenAI {
        /// OpenAI settings
        #[serde(default)]
        settings: crate::openai::Settings,
    },
    #[cfg(feature = "claude")]
    Claude,
}

#[cfg(feature = "generate")]
impl BackendOptions {
    pub fn model_name(&self) -> &str {
        match self {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            BackendOptions::DramaLlama { model, .. } => model
                .file_name()
                .map(|f| {
                    f.to_str().unwrap_or(crate::consts::DEFAULT_MODEL_NAME)
                })
                .unwrap_or(crate::consts::DEFAULT_MODEL_NAME),
            #[cfg(feature = "openai")]
            BackendOptions::OpenAI { settings } => {
                &settings.chat_arguments.model
            }
            #[allow(unreachable_patterns)] // because the number of backends can
            // change based on features and if only one is left, we get a
            // warning we don't want to see.
            _ => crate::consts::DEFAULT_MODEL_NAME,
        }
    }
}

#[cfg(feature = "generate")]
impl BackendOptions {
    pub fn default_for(backend: GenerativeBackend) -> Self {
        match backend {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            GenerativeBackend::DramaLlama => BackendOptions::DramaLlama {
                model: Default::default(),
                predict_options: Default::default(),
                file_dialog: None,
                max_context_size: 128000,
            },
            #[cfg(feature = "ollama")]
            GenerativeBackend::Ollama => BackendOptions::Ollama,
            #[cfg(feature = "openai")]
            GenerativeBackend::OpenAI => BackendOptions::OpenAI {
                settings: Default::default(),
            },
            #[cfg(feature = "claude")]
            GenerativeBackend::Claude => BackendOptions::Claude,
        }
    }

    #[cfg(feature = "openai")]
    pub fn as_openai(&self) -> Option<&crate::openai::Settings> {
        match self {
            BackendOptions::OpenAI { settings } => Some(settings),
            _ => None,
        }
    }
}

// FIXME: This is kind of odd. We have to clone because the predictor takes the
// options by value. We could change the predictor to take a reference but that
// would require a bunch of changes and yet another lifetime on the predictor.
#[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
impl Into<drama_llama::PredictOptions> for &mut BackendOptions {
    fn into(self) -> drama_llama::PredictOptions {
        match self {
            BackendOptions::DramaLlama {
                predict_options, ..
            } => predict_options.clone(),
            #[allow(unreachable_patterns)] // for same reason as above
            _ => Default::default(),
        }
    }
}

#[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
impl Into<std::path::PathBuf> for &mut BackendOptions {
    fn into(self) -> std::path::PathBuf {
        match self {
            BackendOptions::DramaLlama { model, .. } => model.clone(),
            #[allow(unreachable_patterns)] // for same reason as above
            _ => Default::default(),
        }
    }
}

/// Crate settings.
// This is used for App but not much else so we might feature gate this to `gui`
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    /// Default author for new nodes.
    pub default_author: String,
    /// Whether to show the author(s) to the model.
    pub prompt_include_authors: bool,
    /// Whether to show the title to the model.
    pub prompt_include_title: bool,
    #[cfg(feature = "generate")]
    #[serde(default)]
    pub selected_generative_backend: GenerativeBackend,
    #[cfg(feature = "generate")]
    #[serde(default)]
    /// Options for any generative backends that have been used (else Default).
    // We could use a single enum but we want to store options for backends that
    // are not enabled.
    pub backend_options:
        std::collections::HashMap<GenerativeBackend, BackendOptions>,
    #[serde(skip)]
    /// Whether backend switching is pending.
    pub pending_backend_switch: Option<GenerativeBackend>,
}

pub enum Action {
    /// The user has requested to switch generative backends. When the switch is
    /// complete, `Settings::pending_backend_switch` should be set to `None`.
    SwitchBackends {
        /// This backend should be shut down.
        from: GenerativeBackend,
        /// This backend should be started.
        to: GenerativeBackend,
    },
    #[cfg(feature = "openai")]
    OpenAI(crate::openai::SettingsAction),
}

impl Settings {
    #[cfg(feature = "generate")]
    pub fn backend_options(&mut self) -> &mut BackendOptions {
        self.backend_options
            .entry(self.selected_generative_backend)
            .or_insert_with(|| {
                BackendOptions::default_for(self.selected_generative_backend)
            })
    }

    /// Draws generation settings. If there is some additional action the
    /// [`App`] should take, it will return that action.
    ///
    /// [`App`]: crate::app::App
    #[cfg(feature = "generate")]
    pub fn draw_generation_settings(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Action> {
        let mut ret = None;

        // Choose generative backend

        // FIXME: This doesn't display because the backend switch is blocking
        // and by the time the UI is drawn, the backend has already switched.
        // Not sure how to fix this easily.
        if let Some(backend) = &self.pending_backend_switch {
            ui.label(format!(
                "Switching backend to `{}`. Please wait.",
                backend
            ));
        }

        // If there is only one backend, don't show the dropdown.
        if GenerativeBackend::ALL.len() > 1 {
            // allow the user to switch backends
            ui.label("Generative backend:");
            egui::ComboBox::from_label("Backend")
                .selected_text(self.selected_generative_backend.to_string())
                .show_ui(ui, |ui| {
                    for &backend in GenerativeBackend::ALL {
                        let active: bool =
                            self.selected_generative_backend == *backend;

                        if ui
                            .selectable_label(active, backend.to_string())
                            .clicked()
                        {
                            ret = Some(Action::SwitchBackends {
                                from: self.selected_generative_backend,
                                to: *backend,
                            });

                            // We don't immediately switch the backend because we
                            // want to clean up first. The `App` will switch the
                            // `selected_generative_backend` after the cleanup.
                        }
                    }
                });
        }

        // Show the author and title options if the backend supports it. This is
        // outside the match below because two mutable borrows of self are not
        // allowed.
        #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
        if matches!(
            self.selected_generative_backend,
            GenerativeBackend::DramaLlama
        ) {
            ui.checkbox(
                    &mut self.prompt_include_authors,
                    "Include author in prompt sent to model.",
                )
                .on_hover_text_at_pointer("It will still be shown in the viewport. Hiding it can improve quality of generation since models have biases. Does not apply to all backends.");

            ui.checkbox(
                    &mut self.prompt_include_title,
                    "Include title in prompt sent to model.",
                )
                .on_hover_text_at_pointer("It will still be shown in the viewport. Hiding it can improve quality of generation since models have biases. Does not apply to all backends.");
        }

        match self.backend_options() {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            // FIXME: we should do like with `openai` below an have a settings
            // struct with a ui method. This function is getting too long.
            BackendOptions::DramaLlama {
                model,
                predict_options,
                file_dialog,
                max_context_size,
            } => {
                // Choose model
                ui.label(format!("Model: {:?}", model));
                if ui.button("Change model").clicked() {
                    let filter = move |path: &std::path::Path| {
                        path.extension().map_or(false, |ext| ext == "gguf")
                    };
                    let start = if model.as_os_str().is_empty() {
                        None
                    } else {
                        Some(model.clone())
                    };
                    let mut dialog = egui_file::FileDialog::open_file(start)
                        .show_files_filter(Box::new(filter));
                    dialog.open();
                    *file_dialog = Some(dialog);
                }

                if let Some(dialog) = file_dialog {
                    if dialog.show(ui.ctx()).selected() {
                        if let Some(path) = dialog.path() {
                            Self::drama_llama_helper(
                                model,
                                max_context_size,
                                path,
                            )
                        }
                        *file_dialog = None;
                    }
                }

                // Prediction options

                // Stop criteria
                ui.vertical(|ui| {
                    // Because the text edit field escapes special characters,
                    // we'll include a few toggle buttons for common ones and
                    // put them at the top of the list.
                    // TODO: write a custom widget for this.
                    ui.label("Stop at:");
                    let mut skip = 0;
                    ui.horizontal(|ui| {
                        let mut skipping_newline =
                            if !predict_options.stop_strings.is_empty() {
                                if predict_options.stop_strings[0] == "\n" {
                                    skip += 1;
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                        if ui
                            .toggle_value(&mut skipping_newline, "Newline")
                            .clicked()
                        {
                            if skipping_newline {
                                skip += 1;
                                if let Some(s) =
                                    predict_options.stop_strings.get(0)
                                {
                                    debug_assert!(s != "\n")
                                }
                                predict_options
                                    .stop_strings
                                    .insert(0, "\n".to_string());
                            } else {
                                predict_options.stop_strings.remove(0);
                            }
                        }
                    });

                    predict_options.draw_inner(ui);
                });
            }
            #[cfg(feature = "openai")]
            BackendOptions::OpenAI { settings } => {
                if let Some(action) = settings.draw(ui) {
                    ret = Some(Action::OpenAI(action));
                }
            }

            #[allow(unreachable_patterns)] // because same as above
            _ => {}
        }

        ret
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        ui.label("Default author:");
        ui.text_edit_singleline(&mut self.default_author);

        #[cfg(feature = "generate")]
        {
            ui.separator();
            ui.heading("Generation");
            return self.draw_generation_settings(ui);
        }

        #[cfg(not(feature = "generate"))]
        None
    }

    /// This should be called once on startup to configure the backend settings,
    /// for example, validating a local model or fetching a list of models from
    /// OpenAI.
    ///
    /// This function may block briefly, but keep in mind any blocking will slow
    /// down app startup.
    // TODO: see if we can run this in a separate thread, but it makes things
    // much more complicated for little gain.
    #[cfg(feature = "generate")]
    pub fn setup(&mut self) {
        match self.backend_options() {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            BackendOptions::DramaLlama {
                model,
                max_context_size,
                ..
            } => {
                let new = model.clone();
                if model.exists() {
                    Self::drama_llama_helper(model, max_context_size, &new);
                }
            }
            #[cfg(feature = "openai")]
            BackendOptions::OpenAI { ref mut settings } => {
                Self::openai_helper(settings);
            }
            #[allow(unreachable_patterns)] // because same as above
            _ => {}
        }
    }

    /// A helper to configure `drama_llama` settings, avoiding a mutable borrow
    /// of self because we can't call it our draw code otherwise.
    #[cfg(feature = "drama_llama")]
    pub(crate) fn drama_llama_helper(
        model_path: &mut std::path::PathBuf,
        model_context_len: &mut usize,
        desired_path: &std::path::Path,
    ) {
        // Validate the model
        log::debug!("Validating model: {:?}", desired_path);
        if let Some(m) =
            drama_llama::Model::from_file(desired_path.to_path_buf(), None)
        {
            let new_size: usize = m.context_size().try_into().unwrap_or(0);

            if new_size != 0 {
                *model_context_len = m.context_size().max(1) as usize;
                log::debug!("Detected max context size: {}", model_context_len)
            } else {
                log::warn!(
                    "Failed to determine context size for model: {:?}",
                    desired_path
                );
            }

            log::debug!("Model metadata: {:#?}", m.meta());
        } else {
            log::error!("Failed to load model: {:?}", desired_path);
        }

        *model_path = desired_path.to_path_buf();
    }

    /// A helper to configure OpenAI settings
    #[cfg(feature = "openai")]
    pub(crate) fn openai_helper(settings: &mut crate::openai::Settings) {
        if let Err(e) = settings.fetch_models_sync(None) {
            // TODO: we could use a concrete error type here because it will
            // tell us if the error is related to the API key or not. If it is
            // related to the API key, we should show a message to the user in
            // the UI to prompt them to set the API key, and then retry this.
            log::error!("Failed to fetch models from OpenAI because: {}", e);
            log::error!("Make sure you have an API key set.");
        }
    }
}
