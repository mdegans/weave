#[macro_export]
#[cfg(feature = "gui")]
macro_rules! icon {
    ($ui:expr, $path:expr, $size:expr) => {
        $ui.add(egui::Image::new(egui::include_image!($path)).max_height($size))
    };
    ($ui:expr, $path:expr) => {
        icon!($ui, $path, 12.0)
    };
}

#[macro_export]
#[cfg(feature = "gui")]
macro_rules! button {
    ($ui:expr, $path:expr) => {
        $ui.add(egui::Button::image(egui::include_image!($path)))
    };
}
