#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), deny(warnings))] // Forbid warnings in release builds
#![warn(clippy::all, rust_2018_idioms)]

#[cfg(feature = "gui")]
pub mod app;

#[cfg(feature = "openai")]
pub(crate) mod openai;

#[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
pub(crate) mod drama_llama;

pub mod consts;
pub mod node;
pub mod settings;
pub mod story;

// ----------------------------------------------------------------------------
// When compiling for web:

#[cfg(feature = "gui")]
#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::{self, prelude::*};

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn start(canvas_id: &str) -> Result<(), eframe::wasm_bindgen::JsValue> {
    let app = app::App::default();
    eframe::start_web(canvas_id, Box::new(app))
}
