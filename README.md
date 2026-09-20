# NetDraw

NetDraw is a lightweight desktop application for creating and editing network diagrams. It provides a focused canvas for drawing connected elements, labeling them, and exporting clean figures for reports and presentations.

> **Pre-release:** NetDraw is currently under active development. Features, behavior, and the `.netdraw` file format may change between releases.

## Features

- Draw lines, resistors, and prescribed-flow elements
- Snap endpoints together to create connected networks
- Select and move individual or connected endpoints
- Add labels to elements, junctions, and unconnected terminals
- Use configurable fonts, subscripts, and superscripts in labels
- Save editable drawings as `.netdraw` project files
- Open existing `.netdraw` drawings and continue editing
- Export finished diagrams as SVG with embedded fonts
- Protect unsaved work when opening, creating, or closing drawings

## Getting started

NetDraw is written in Rust. Install a current Rust toolchain, clone the repository, and run:

```sh
cargo run --release
```

For a faster development build, use:

```sh
cargo run
```

## Using NetDraw

Choose a drawing tool from the toolbar, then drag across the canvas to create an element. Nearby compatible endpoints snap together automatically.

Use the selection tool to select and move endpoints. Connected endpoints can be moved together, while the label and erase tools let you annotate or remove parts of the drawing.

The **File** menu provides two separate workflows:

- **Save** and **Open** use the editable `.netdraw` project format.
- **Export** creates an SVG for use in other applications.

Keep the `.netdraw` file if you expect to edit a diagram again; an exported SVG is intended as final output.

## Labels

Labels support simple subscript and superscript notation:

- `R_1` produces a subscript.
- `x^2` produces a superscript.
- `R_(total)` groups multiple subscript characters.
- A backslash escapes a marker when a literal `_` or `^` is needed.

The label typography dialog lets you select a bundled font and adjust its size. Selected fonts are embedded in exported SVG files for consistent rendering.

## Project status

This repository contains an early preview intended for testing and feedback. Before relying on a pre-release build, keep backup copies of important drawings. Bug reports and focused improvement suggestions are welcome through GitHub Issues.

## Development

Run the test suite with:

```sh
cargo test
```

The application is built with [egui/eframe](https://github.com/emilk/egui) and uses [rfd](https://github.com/PolyMeilex/rfd) for native file dialogs.

## License

NetDraw is free and open-source software released under the [MIT License](LICENSE).
