//! NetDraw desktop executable. See `docs/architecture.md` for the module map.

// On Windows, release builds use the GUI subsystem so no console window opens next to
// the app. Debug builds keep the console, so `cargo run` still shows panics and logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
