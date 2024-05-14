#[cfg(feature = "generate")]
use crate::settings::GenerativeBackend;

// Story options

/// What to use if the story has no title.
pub const DEFAULT_TITLE: &str = "Untitled";
/// What to use if the story has no author.
pub const DEFAULT_AUTHOR: &str = "Anonymous";
/// What to use if the model name cannot be determined.
pub const DEFAULT_MODEL_NAME: &str = "AI";

// Generative options

/// All the generative backends that can be used.
#[cfg(feature = "generate")]
pub const GENERATIVE_BACKENDS: &[&GenerativeBackend] = &[
    #[cfg(feature = "drama_llama")]
    &GenerativeBackend::DramaLlama,
    #[cfg(feature = "ollama")]
    &GenerativeBackend::Ollama,
    #[cfg(feature = "openai")]
    &GenerativeBackend::OpenAI,
    #[cfg(feature = "claude")]
    &GenerativeBackend::Claude,
];

#[cfg(feature = "generate")]
const _: () = check_backends();
#[cfg(feature = "generate")]
const fn check_backends() {
    // a descriptive error message is better than a panic
    if GENERATIVE_BACKENDS.is_empty() {
        panic!(
            "There must be at least one generative backend feature enabled to use the `generate` feature."
        );
    }
}

/// The default generative backend to use.
// There must be at least one backend or the app will not compile.
#[cfg(feature = "generate")]
pub const DEFAULT_GENERATIVE_BACKEND: &GenerativeBackend =
    GENERATIVE_BACKENDS[0];
