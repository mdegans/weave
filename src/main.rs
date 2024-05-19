//! Native entrypoints for the Weave application.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), deny(warnings))]
#![warn(clippy::all, rust_2018_idioms)]

#[cfg(all(feature = "gui", not(target_arch = "wasm32")))]
fn main() {
    use eframe::egui::Visuals;
    use weave::app::App;

    env_logger::init();

    eframe::run_native(
        "Weave",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(Visuals::dark());
            Box::new(App::new(cc))
        }),
    )
    .expect("Failed to run native example");
}

#[cfg(all(not(feature = "gui"), not(target_arch = "wasm32")))]
fn main() {
    println!("This example requires the `gui` feature.");
}
