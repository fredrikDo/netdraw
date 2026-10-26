# Development and architecture

NetDraw is a Rust desktop application using egui/eframe. The executable owns one
canvas, with geometry and labels stored in a `Drawing` snapshot. The solver is
local, deterministic, and independent of rendering and file I/O.

## Build and verify

Run from the repository root with a current stable Rust toolchain:

```sh
cargo run
cargo run --release
cargo test
cargo fmt --check
cargo doc --no-deps --document-private-items
```

The first build needs registry access to download Cargo dependencies. Native GUI
and clipboard dependencies depend on the target platform. Windows icon resources
also require a Windows SDK resource compiler or MinGW `windres`. Bundled fonts
need no download at runtime.

Tests live beside their implementation under `#[cfg(test)]`. The files in
`tests/fixtures/` are regression drawings loaded with `include_str!`; tests do not
read a user's Downloads folder. Coverage includes persistence, editing history,
geometry, label layout, SVG/raster export, network validation, exact arithmetic,
and factored formulas checked against numerical values.

## Module map

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Module declarations and desktop entry point |
| `src/canvas.rs` | UI state, tools, selection, editing, history, dialogs, file/clipboard actions |
| `src/element.rs` | Element geometry, endpoint hit testing, snapping, junction and terminal discovery |
| `src/document.rs` | Drawing snapshots and versioned JSON conversion/validation |
| `src/label.rs` | Font assets, script notation, text measurement, and label placement |
| `src/renderer.rs` | Painting diagram geometry and editing handles in egui |
| `src/exporter.rs` | Cropped SVG output, embedded fonts, and clipboard rasterization |
| `src/icon.rs` | Rasterizing the bundled SVG into the window icon |
| `src/network_solver.rs` | Translating the selected drawing component into a mathematical system |
| `src/symbolic.rs` | Exact polynomial/rational arithmetic, cancellation, and linear elimination |
| `src/symbolic_format.rs` | Factored presentation trees, precedence, powers, and determinant layout |
| `build.rs` | Windows icon resource compilation |

The `Resistor` variant is a historical geometry/file-format name. Its mathematical
label is a conductance. Keep that distinction explicit when modifying solver code.

## Editing and persistence

The canvas stores element labels in a parallel vector: `labels[i]` belongs to
`elements[i]`. Node and boundary labels attach to positions. Geometry operations
must preserve label correspondence, move position labels with their endpoints,
and remove labels whose targets disappear.

Completed drawings use canvas coordinates. Rendering applies the current screen
offset; screen positions must not be saved or passed into topology construction.
Snapping produces coincident coordinates, and topology uses exact position
identity rather than a second distance threshold.

`Canvas::drawing` extracts persistent state. Selection, tool mode, unfinished
strokes, open dialogs, and solved-expression snapshots are transient. Applying a
drawing clears interaction state so references to old element indices cannot
survive an undo, redo, open, or new-document action.

History stores drawing snapshots, with gesture handling avoiding a separate undo
entry for every frame of a drag. Unsaved status compares the current snapshot with
the last successful save. Saving writes a sibling temporary file before renaming
it over the destination. Serialization itself is separate from those I/O actions.
See [the file format](file-format.md).

## Rendering and export

Canvas painting and export share geometry and label layout helpers. The SVG
export embeds the selected bundled font and computes bounds from geometry and
labels. Clipboard output rasterizes that SVG onto white, excludes editor overlays,
and rejects excessive raster dimensions. Keep the clipboard owner alive on Linux
so another application can request the image later.

When changing an element's shape, check canvas painting, exported SVG, and export
bounds together. When changing text layout, check both canvas and export to avoid
cropping or misplaced scripts.

## Symbolic solving pipeline

### 1. Build topology

`drawing_topology` gathers connectable endpoints and merges the endpoints of ideal
lines. A node identifier is a representative endpoint index; identifiers can have
gaps after merging. `connected_component` traverses conductance edges from the
selection. Only that component is validated and solved.

Only a prescribed-flow arrow's tip enters topology. The arrow is an external
injection, not a two-terminal conductance. Junction dots are a visual aid and do
not define the solver graph.

### 2. Identify boundaries and validate

`boundary_pressure` reads terminal labels as known values. An unlabeled degree-one
node without prescribed inflow is considered a missing boundary label; a node with
prescribed inflow can instead be an unknown node. Connected terminal labels with
different values are rejected. Internal node labels only name results.

`solve_selected` validates the selection, component labels, boundary presence, and
six-unknown-node limit before generating the result. It returns a display string
or a user-readable error and never mutates the drawing.

### 3. Assemble equations

For each edge of conductance `G` from unknown node `i` to node `j`:

- Add `G` to diagonal coefficient `A[i][i]`.
- If `j` is unknown, subtract `G` from `A[i][j]`.
- If `j` is known, add `G*p_j` to `b[i]`.
- Add prescribed inflows directly to `b[i]`, independently of `p_i`.

This produces `A*x = b`. Edges whose endpoints are the same mathematical node
carry no node difference and do not contribute. `build_equations_with_sources`
accepts a source-value callback so physical equations and unit-response equations
share the same assembly logic.

### 4. Separate a shared reference

If a boundary label appears on multiple mathematical nodes, `reference_solution`
chooses a most frequent shared label. It solves a unit response for each other
boundary label and for each distinct prescribed-flux label. The final expression
is the reference plus weighted boundary differences and independent fluxes.
Repeated flux labels are combined into one source response. Fluxes on known
boundaries do not affect unknown values.

If no boundary label repeats, solve the original equations directly. Reference
selection and source collection use ordered maps/sets for deterministic output.

### 5. Exact arithmetic and cancellation

A `Polynomial` maps a sorted vector of symbol names to an arbitrary-precision
integer coefficient. Repeated symbols represent powers; an empty vector is a
constant term. Zero coefficients are removed. An `Expression` is a numerator and
denominator of such polynomials.

Arithmetic combines terms exactly, cancels numeric content and common monomials,
and attempts cancellation of polynomial factors encountered in earlier operations.
Exact multivariate long division verifies cancellation; a remainder or work-limit
failure leaves the original factor uncancelled. Candidate tracking is bounded and
is not a general polynomial GCD or factorization algorithm.

Current work limits in `symbolic.rs`:

| Limit | Value / meaning |
| --- | --- |
| Polynomial terms | 2,048 retained terms |
| Multiplication precheck | At most 8,192 input term pairs |
| Exact-division loop | At most 2,048 iterations per attempt |
| Tracked factor candidates | At most 64 per expression |

`solve` uses Gaussian elimination with row swapping for structurally zero pivots,
then back substitution. It forms no matrix inverse. Symbolic parameters are
handled generically: positivity/finiteness is assumed by the model, and the code
does not enumerate special parameter values or track all excluded denominators.
A structurally singular system is reported as lacking a unique solution.

### 6. Choose readable output

`compact_solution` first obtains the cancelled exact solution. For polynomial
systems it also constructs a factored ratio of determinants using Cramer's rule
for presentation. Sparse Laplace expansion chooses a row with few nonzero entries.
The formatter keeps sums/products, groups repeated factors into powers, and adds
parentheses according to precedence. It chooses the shorter rendered candidate.

`Formula` is a presentation tree, not an alternative algebra engine. It does not
change the canonical arithmetic representation. Numerical regression tests compare
factored trees with exact results and analytic transfer coefficients. The six-node
cap also bounds the cost of determinant-based presentation.

## Guidelines for changes

Keep UI side effects in the canvas. Network conversion should consume snapshots
and return values; symbolic arithmetic should not depend on egui or drawing
coordinates. Give each helper one responsibility, and comment assumptions,
invariants, and reasons for an algorithm rather than restating assignments.

For solver changes, add regression drawings or small equation tests with known
answers. Check flux independence, shared boundaries, singular systems, and
numerical agreement of formatted expressions. For layout changes, inspect the
running application and exported image in addition to automated tests.
