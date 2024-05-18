use serde::{Deserialize, Serialize};

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
impl Default for GenerativeBackend {
    fn default() -> Self {
        *crate::consts::DEFAULT_GENERATIVE_BACKEND
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
}

impl Settings {
    pub fn new() -> Self {
        Self {
            default_author: "Anonymous".to_string(),
            prompt_include_authors: true,
            prompt_include_title: true,
            #[cfg(feature = "generate")]
            selected_generative_backend: GenerativeBackend::default(),
            #[cfg(feature = "generate")]
            backend_options: std::collections::HashMap::new(),
        }
    }

    #[cfg(feature = "generate")]
    pub fn backend_options(&mut self) -> &mut BackendOptions {
        self.backend_options
            .entry(self.selected_generative_backend)
            .or_insert_with(|| {
                BackendOptions::default_for(self.selected_generative_backend)
            })
    }

    #[cfg(feature = "generate")]
    pub fn draw_generation_settings(&mut self, ui: &mut egui::Ui) {
        // Choose generative backend

        use std::num::NonZeroU128;

        ui.label("Generative backend:");
        egui::ComboBox::from_label("Backend")
            .selected_text(self.selected_generative_backend.to_string())
            .show_ui(ui, |ui| {
                for &backend in crate::consts::GENERATIVE_BACKENDS {
                    // The linter is wrong. `backend` is used below.
                    #[allow(unused_variables)]
                    let active: bool =
                        matches!(self.selected_generative_backend, backend);

                    if ui
                        .selectable_label(active, backend.to_string())
                        .clicked()
                    {
                        // We need to shutdown the worker if we're changing
                        // backends because the worker is tied to the backend.
                        // FIXME: Because the app has the worker, we should
                        // return something indicating the worker should be
                        // restarted. I can't think of another way. If we do
                        // that, we can't change it immediatly here, but should
                        // return the selected backend and then change it in the
                        // App::update method. It's a bit of a mess.
                        // Alternatively we could move the workers into the
                        // settings struct. It's a bit odd but it would work and
                        // might be cleaner. As it stands, a running worker for
                        // a given backend will keep running until the app is
                        // closed. That might not be terrible, but some backends
                        // can use a lot of resources, like the local models.
                        self.selected_generative_backend = *backend;
                    }
                }
            });

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
                use drama_llama::PredictOptions;

                // Choose model
                ui.label(format!("Model: {:?}", model));
                if ui.button("Change model").clicked() {
                    let filter = move |path: &std::path::Path| {
                        path.extension()
                            .and_then(std::ffi::OsStr::to_str)
                            .map(|ext| ext == "gguf")
                            .unwrap_or(false)
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
                    }
                }

                // Prediction options

                // FIXME: Figure out how to get the tooltip to show up for the
                // `horizontal_wrapped` widget. The docs say "The returned
                // Response will only have checked for mouse hover but can be
                // used for tooltips (on_hover_text)" but this doesn't appear to
                // work or I'm holding it wrong.

                // context window size
                ui.horizontal_wrapped(|ui| {
                    ui.label("Context:").on_hover_text_at_pointer("The total number of tokens in the context window. This includes the prompt and the generated text. The limit is set by the model. In general, set this as high as your model and memory can handle.");

                    let mut n = predict_options.n.get().min(*max_context_size).max(1);
                    ui.add(
                        egui::widgets::DragValue::new(&mut n)
                            .clamp_range(1..=*max_context_size),
                    );
                    // The min and max is necessary because the widget doesn't
                    // actually clamp the value correctly. If one drags the
                    // value all the way to the left, it will set n to 0 :/
                    predict_options.n = n.min(*max_context_size).max(256).try_into().unwrap();
                });

                // Random seed
                ui.horizontal_wrapped(|ui| {
                    let mut random = predict_options.seed.is_none();
                    ui.toggle_value(&mut random, "Random seed").on_hover_text_at_pointer("Use a random seed for predictions. Click to set a specific seed.");
                    predict_options.seed = if random {
                        None
                    } else {
                        // unwrap cannot panic because we already checked for None
                        // FIXME: were truncating here. This isn't great, but
                        // the widget doesn't support u128.
                        let mut seed: u64 =
                            predict_options.seed.unwrap_or(PredictOptions::DEFAULT_SEED).get()
                                as u64;
                        ui.add(
                            egui::widgets::DragValue::new(&mut seed)
                                .clamp_range(1..=std::usize::MAX),
                        );

                        Some(NonZeroU128::try_from(seed as u128).unwrap())
                    };
                });

                // Stop criteria
                ui.vertical(|ui|{
                    // Because the text edit field escapes special characters,
                    // we'll include a few toggle buttons for common ones and
                    // put them at the top of the list.
                    // TODO: write a custom widget for this.
                    ui.label("Stop at:");
                    let mut skip = 0;
                    ui.horizontal(|ui| {
                        let mut skipping_newline = if !predict_options.stop_strings.is_empty() {
                            if predict_options.stop_strings[0] == "\n" {
                                skip += 1;
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if ui.toggle_value(&mut skipping_newline, "Newline").clicked() {
                            if skipping_newline {
                                skip += 1;
                                if let Some(s) = predict_options.stop_strings.get(0) {
                                    debug_assert!(s != "\n")
                                }
                                predict_options.stop_strings.insert(0, "\n".to_string());
                            } else {
                                predict_options.stop_strings.remove(0);
                            }
                        }
                    });

                    ui.label("Stop strings:").on_hover_text_at_pointer("Stop generating when any of these strings are predicted. Note that escape sequences are not currently supported.");

                    let mut remove = None;

                    for (i, stop) in predict_options.stop_strings.iter_mut().enumerate().skip(skip) {
                        ui.horizontal_wrapped(|ui| {
                            // This escapes special characters, which is not
                            // what we want. bluh.
                            ui.text_edit_singleline(stop);
                            if ui.button("X").clicked() {
                                remove = Some(i);
                            }
                        });
                    }

                    if let Some(i) = remove {
                        predict_options.stop_strings.remove(i);
                        remove = None;
                    }

                    if ui.button("Add stop string").clicked() {
                        predict_options.stop_strings.push(Default::default());
                    }

                    ui.label("Stop token sequences:").on_hover_text_at_pointer("Stop generating when any of these token sequences are predicted. Note that any model-specific sequences will be added automatically on generation and not shown here.");
                    for (i, seq) in predict_options.stop_sequences.iter_mut().enumerate() {
                        // This might not work very well because the edit will
                        // be cleared when the string is updated and this
                        // happens at every frame. We could use a separate
                        // buffer, but it would have to be associated with the
                        // one that is being edited. If this doesn't work we can
                        // have a delete button and a separate add button with a
                        // text field.
                        let mut s = int_vec_to_string(seq);
                        ui.horizontal_wrapped(|ui| {
                            ui.text_edit_singleline(&mut s);
                            if ui.button("X").clicked() {
                                remove = Some(i);
                            }
                        });
                        *seq = string_to_int_vec(&s);
                    }
                });

                // TODO: Add ui for options. This is perhaps better done in
                // the drama_llama crate.
            }
            #[cfg(feature = "openai")]
            BackendOptions::OpenAI { settings } => {
                settings.ui(ui);
            }

            #[allow(unreachable_patterns)] // because same as above
            _ => {}
        }
    }

    #[cfg(feature = "gui")]
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        ui.label("Default author:");
        ui.text_edit_singleline(&mut self.default_author);

        ui.checkbox(
            &mut self.prompt_include_authors,
            "Include author in prompt sent to model.",
        )
        .on_hover_text_at_pointer("It will still be shown in the viewport. Hiding it can improve quality of generation since models have biases.");

        ui.checkbox(
            &mut self.prompt_include_title,
            "Include title in prompt sent to model.",
        )
        .on_hover_text_at_pointer("It will still be shown in the viewport. Hiding it can improve quality of generation since models have biases.");

        #[cfg(feature = "generate")]
        {
            self.draw_generation_settings(ui);
        }
    }

    /// Configure model-specific settings when a local model is loaded. It will:
    /// * Set the model path if the model is valid.
    /// * Set the maximum context size if the model is valid.
    ///
    /// This can block, but only briefly. Mmap is used by default and we're just
    /// reading the metadata. Call it on setup, from the worker thread, or from
    /// the main thread if it's really necessary.
    // Like above in the draw code. If we're changing the model we do need to
    // validate it and the api doesn't allow us to do that without blockign
    // currently.
    #[cfg(feature = "generate")]
    pub fn configure_for_new_local_model(&mut self, path: &std::path::Path) {
        match self.backend_options() {
            #[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
            BackendOptions::DramaLlama {
                model,
                predict_options,
                file_dialog,
                max_context_size,
            } => {
                Self::drama_llama_helper(model, max_context_size, path);
            }
            _ => {}
        }
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
                predict_options,
                file_dialog,
                max_context_size,
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

#[cfg(feature = "gui")]
fn int_vec_to_string(vec: &[i32]) -> String {
    vec.iter()
        .map(|&i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(feature = "gui")]
fn string_to_int_vec(s: &str) -> Vec<i32> {
    s.split(',').filter_map(|s| s.trim().parse().ok()).collect()
}
