# The `.netdraw` file format

A `.netdraw` file is UTF-8 JSON. `document.rs` converts between the runtime
`Drawing` and private serialization types. The current format version is **1**;
other versions are rejected. The application remains pre-release, so retain
backups when moving between builds.

## Top-level fields

| Field | Meaning |
| --- | --- |
| `version` | Integer format version, currently `1` |
| `elements` | Ordered array of zigzag and prescribed-flow geometry |
| `labels` | String array with exactly one entry per element, in the same order |
| `node_labels` | Position-attached labels that name internal nodes |
| `terminal_labels` | Position-attached labels used as known boundary values |
| `label_style` | Shared font and size for drawing labels |

Each element has `type`, `start`, and `end`. A position is `[x, y]` in canvas
coordinates. Valid types are `resistor` and `prescribed_flow`. Files containing the
former `line` (plain wire) type are rejected with an explanatory error.
`resistor` stores the zigzag geometry; the solver interprets its label as
conductance. Only a `prescribed_flow` element's `end` is mathematically connected.

A position label has `position` and `text`. An empty string in `labels` is valid
for an unfinished drawing, but solving requires appropriate nonempty labels.

## Minimal example

```json
{
  "version": 1,
  "elements": [
    {"type": "resistor", "start": [100.0, 100.0], "end": [300.0, 100.0]}
  ],
  "labels": ["G_1"],
  "node_labels": [],
  "terminal_labels": [
    {"position": [100.0, 100.0], "text": "P_in"},
    {"position": [300.0, 100.0], "text": "P_out"}
  ],
  "label_style": {"font": "inter", "size": 22.0}
}
```

Font names are `inter`, `source_serif_4`, `ibm_plex_mono`, and
`atkinson_hyperlegible`. Some retired names are accepted as aliases and mapped to
current fonts; see [font sources and replacements](../assets/fonts/README.md).

## Validation and compatibility

Loading rejects malformed JSON, unsupported versions, mismatched label/element
counts, nonfinite coordinates, and font sizes outside `8..=48`. Loading a drawing
does not require it to be a solvable network. Topology, missing boundary labels,
and symbolic limits are validated when Solve is clicked.

Connectivity is implicit in endpoint coordinates. There are no saved node IDs,
edge values, parsed equations, or solver caches. Selection, grid visibility,
undo/redo stacks, open dialogs, and calculated expressions are not persisted.

Keep the serialized geometry separate from egui internals so the file format can
be changed deliberately. Changes to the schema should consider version handling,
conversion code, validation, and round-trip tests together. Exported SVG and
clipboard images are output artifacts, not editable project files.
