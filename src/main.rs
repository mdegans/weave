#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), deny(warnings))] // Forbid warnings in release builds
#![warn(clippy::all, rust_2018_idioms)]

// When compiling natively:
#[cfg(feature = "gui")]
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use eframe::egui::Visuals;
    use weave::app::App;

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

#[cfg(not(feature = "gui"))]
fn main() {
    println!("This example requires the `gui` feature.");
}
