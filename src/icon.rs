use eframe::egui;

pub fn app_icon() -> egui::IconData {
    let tree = resvg::usvg::Tree::from_str(
        include_str!("../assets/icons/netdraw.svg"),
        &resvg::usvg::Options::default(),
    )
    .expect("Invalid bundled application icon");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(256, 256).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    egui::IconData {
        rgba: pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect(),
        width: pixmap.width(),
        height: pixmap.height(),
    }
}
