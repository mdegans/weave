//! Weave is primarily a binary crate, but has reusable components that can be
//! used in other story-writing projects.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), deny(warnings))] // Forbid warnings in release builds
#![warn(clippy::all, rust_2018_idioms)]

/// [`egui`] [`App`]` for the Weave application.
#[cfg(feature = "gui")]
pub mod app;

/// OpenAI generative [`Worker`]. [`Request`]s are sent to the worker and
/// [`Response`]s are received.
#[cfg(feature = "openai")]
pub(crate) mod openai;

/// [`drama_llama`] generative [`Worker`]. [`Request`]s are sent to the worker
/// and [`Response`]s are received.
#[cfg(all(feature = "drama_llama", not(target_arch = "wasm32")))]
pub(crate) mod drama_llama;

/// Crate-wide constants.
pub mod consts;
/// Contains [`Node`] and associated types such as [`Meta`].
pub mod node;
/// Contains a branching [`Story`] (a tree of [`Node`]s).
pub mod story;

// wasm entrypoints:

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
