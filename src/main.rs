//! NetDraw desktop executable. See `docs/architecture.md` for the module map.

mod canvas;
mod document;
mod element;
mod exporter;
mod icon;
mod label;
mod network_solver;
mod renderer;
mod symbolic;
mod symbolic_format;

fn main() -> eframe::Result<()> {
    canvas::run()
}
