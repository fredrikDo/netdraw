# NetDraw

NetDraw is a Rust desktop application for drawing conductance networks, solving
node values symbolically, and exporting clean diagrams for reports and presentations.
It can model steady linear heat, pressure, or similar potential-and-flux networks.

> **Pre-release:** NetDraw is currently under active development. Features, behavior, and the `.netdraw` file format may change between releases.

## Documentation

- [Solving networks](docs/solving.md): workflow, equations, examples, limits, and errors.
- [Development and architecture](docs/architecture.md): module responsibilities and solver algorithms.
- [Project file format](docs/file-format.md): JSON schema, validation, and compatibility.
- [Bundled fonts](assets/fonts/README.md): sources, licenses, and replacement mappings.

## Features

- Draw ideal connections, conductance zigzags, and prescribed-flow elements
- Solve a selected node symbolically using exact arithmetic, with up to six unknown nodes
- Snap endpoints together to create connected networks
- Select and move individual or connected endpoints
- Add labels to elements, junctions, and unconnected terminals
- Use configurable fonts, subscripts, and superscripts in labels
- Save editable drawings as `.netdraw` project files
- Open existing `.netdraw` drawings and continue editing
- Copy tightly cropped diagram images to the clipboard on Linux and Windows
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

Toggle **Grid** in the toolbar to show a gray grid with 20 px spacing and snap drawing endpoints to its intersections. Moving selected endpoints also snaps the selection anchor to the grid. Use **Snap selected endpoints to grid**, beside the Grid button, to snap each selected endpoint to its nearest intersection even when the grid is hidden. The grid is only a visual aid and is excluded from SVG exports.

Use the selection tool to select and move endpoints. Connected endpoints can be moved together, while the label and erase tools let you annotate or remove parts of the drawing.

Drag a selection box around two or more endpoints, then use **Align horizontally** or **Align vertically** in the toolbar. Alignment uses the center of the selection and keeps the endpoints selected. When Grid is enabled, aligned endpoints snap to grid intersections. **Equalize connected lengths** also snaps its resulting endpoints to the grid; rounding to intersections can make the final lengths approximate.

Use **Edit → Undo** (`Ctrl+Z`) and **Edit → Redo** (`Ctrl+Y`) to reverse or restore
completed edits. With endpoints selected, `Delete` or `Backspace` removes the
attached elements; text fields handle their own editing keys while focused.

The **File** menu provides two separate workflows:

- **Save** and **Open** use the editable `.netdraw` project format.
- **Export** creates an SVG for use in other applications.

Click **Copy image** to paste a tightly cropped image of the complete drawing, including labels, into a document. The image has a white background and excludes the grid, selection handles, and unused canvas. Keep NetDraw open until you paste on Linux; clipboard persistence after closing depends on your clipboard manager. Linux supports X11 and Wayland compositors with clipboard data-control support.

Keep the `.netdraw` file if you expect to edit a diagram again; an exported SVG is intended as final output.

## Solving a node

See the [solver guide](docs/solving.md) for worked examples and troubleshooting.


Label every conductance and prescribed-flow arrow in the selected connected network.
Label each free terminal with its known pressure (for example `P_in` or `P_out`).
Labels are literal symbolic names, not parsed formulas or numerical values.
Use the selection tool to mark one node, then click **Solve selected node** (`=`)
in the toolbar. The result window shows a selectable expression and a copy button.
A node label is optional and names the result when present.

Zigzag element labels represent conductance `G`: edge flux is `G * (p_i - p_j)`.
Prescribed flux `Q` is an independent source, so conservation gives
`sum(G_ij * (p_i - p_j)) = Q_i`. For one conductance connected to a known
boundary, the result is `p = p_boundary + Q / G`.
Existing drawings retain their labels; labels formerly used as resistance names
must now be interpreted or relabeled as conductances for solving.

The solver assumes positive, finite conductances, conservation of flow, and positive
injection at each flow arrow tip. Elements connect only where endpoints coincide;
crossings do not connect. Every free zigzag endpoint (marked with a terminal tick)
must be labeled as a pressure boundary.

The selected connected network may contain up to six unknown pressure nodes and
must have a known pressure boundary. Unrelated disconnected drawings are ignored.
Exact symbolic arithmetic runs locally in Rust, using fraction-free elimination so
intermediate expressions stay no larger than the determinants of the system.
Simplification cancels numeric, monomial, and shared polynomial factors; general
polynomial factoring is not included. Very dense networks may still reach a size
limit before the node limit. Labels that are plain numbers (such as `0`) are
rejected; use symbol names such as `T_0`.

When several boundaries share a label, the most frequent shared value becomes the
reference. The result is expressed as that reference plus weighted boundary
differences and independent prescribed fluxes. For example, boundaries `T_0` and
`T_e` can give `T = T_0 + alpha*(T_e - T_0)`, with the conductance expression for
`alpha` written directly in the result. Sums, products, and powers are preserved
when they give a shorter expression than the expanded result.

The result is a snapshot; click Solve again after editing the drawing.

## Application icon

The application uses the blue resistor icon in `assets/icons/netdraw.svg`. The window icon is embedded on all platforms, and Windows builds also embed `assets/icons/netdraw.ico` into the executable (using the Windows SDK resource compiler or MinGW `windres`).

Linux desktop launchers, including Wayland desktops, use the supplied desktop entry and installed icon. After `cargo build --release`, install them for your user with:

```sh
install -Dm755 target/release/netdraw ~/.local/bin/netdraw
install -Dm644 assets/icons/netdraw.svg ~/.local/share/icons/hicolor/scalable/apps/netdraw.svg
install -Dm644 assets/icons/netdraw.png ~/.local/share/icons/hicolor/256x256/apps/netdraw.png
install -Dm644 packaging/linux/netdraw.desktop ~/.local/share/applications/netdraw.desktop
```

Ensure `~/.local/bin` is in your desktop session's `PATH`, then launch **NetDraw** from the application menu.

## Labels

Labels support simple subscript and superscript notation:

- `R_1` produces a subscript.
- `x^2` produces a superscript.
- `R_(total)` groups multiple subscript characters.
- A backslash escapes a marker when a literal `_` or `^` is needed.

The label typography dialog lets you choose **Inter** (the default), **Source Serif 4**, **IBM Plex Mono**, or **Atkinson Hyperlegible**, and adjust the label size. These regular upright fonts are bundled with the app and embedded in exported SVG files for consistent rendering.

Drawings using removed fonts open with a replacement from the new selection. See [bundled fonts](assets/fonts/README.md) for sources, licenses, and the replacement mapping.

## Project status

This repository contains an early preview intended for testing and feedback. Before relying on a pre-release build, keep backup copies of important drawings. Bug reports and focused improvement suggestions are welcome through GitHub Issues.

## Development

See [development and architecture](docs/architecture.md) for the code map, invariants,
and symbolic solving pipeline. The project keeps UI handling, topology conversion,
exact algebra, and expression formatting in separate modules.

Run the test suite with:

```sh
cargo test
cargo fmt --check
cargo doc --no-deps --document-private-items
```

The application is built with [egui/eframe](https://github.com/emilk/egui) and uses [rfd](https://github.com/PolyMeilex/rfd) for native file dialogs.

## License

NetDraw is free and open-source software released under the [MIT License](LICENSE). Bundled fonts use the SIL Open Font License 1.1; see [font licenses and sources](assets/fonts/README.md).
