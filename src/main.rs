mod canvas;
mod document;
mod element;
mod exporter;
mod icon;
mod label;
mod renderer;

fn main() -> eframe::Result<()> {
    canvas::run()
}
