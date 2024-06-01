//! Native entrypoints for the Weave application.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), deny(warnings))]
#![warn(clippy::all, rust_2018_idioms)]

use egui::IconData;

fn load_icon() -> std::sync::Arc<IconData> {
    // Uncomment to generate icon.raw file. The `image` crate will need to be
    // added as a dependency in Cargo.toml.

    // let img =
    //     image::load_from_memory(include_bytes!("../resources/icon.512.png"))
    //         .unwrap();
    // assert!(
    //     img.height() == 512 && img.width() == 512,
    //     "Icon must be 512x512"
    // );
    // let raw = img.to_rgba8().into_raw();
    // // write raw to file. then we can remove dependency on image
    // const RAW_ICON_FILENAME: &str = "resources/icon.512.raw";
    // std::fs::write(RAW_ICON_FILENAME, &raw).unwrap();
    const BYTES: &[u8] = include_bytes!("../resources/icon.512.raw");
    static_assertions::const_assert_eq!(BYTES.len(), 512 * 512 * 4);

    IconData {
        width: 512,
        height: 512,
        rgba: BYTES.to_vec(),
    }
    .into()
}

#[cfg(all(feature = "gui", not(target_arch = "wasm32")))]
fn main() {
    use eframe::egui::Visuals;
    use egui::ViewportBuilder;
    use weave_writer::app::App;

    env_logger::init();

    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = ViewportBuilder::default().with_icon(load_icon());

    eframe::run_native(
        "Weave",
        native_options,
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
