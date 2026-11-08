//! Bundled typography, display-only label scripts, and collision-aware placement.
//!
//! Geometry and exports share these measurements. Script notation changes
//! appearance only; it never parses a symbolic solver expression.

use eframe::egui;
use std::sync::Arc;

use crate::element::Element;

/// Minimum gap kept between a label rectangle and any element's centre line.
const ELEMENT_CLEARANCE: f32 = 8.0;
/// Script spans are drawn at this fraction of the base size, on canvas and in SVG.
const SCRIPT_SCALE: f32 = 0.7;
/// Padding added to measured text so neighbouring labels never touch.
const LABEL_PADDING: egui::Vec2 = egui::vec2(4.0, 4.0);

/// Bundled label typefaces. Serialized names live in `document::SavedFont`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelFont {
    Inter,
    SourceSerif4,
    IbmPlexMono,
    AtkinsonHyperlegible,
}

/// Drawing-wide label typography; every label uses the same font and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelStyle {
    pub font: LabelFont,
    pub size: f32,
}

/// Vertical placement of one run of label text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptPosition {
    Normal,
    Subscript,
    Superscript,
}

/// A run of label text sharing one script position, produced by [`parse_label`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSpan {
    pub text: String,
    pub position: ScriptPosition,
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self {
            font: LabelFont::Inter,
            size: 22.0,
        }
    }
}

impl LabelStyle {
    /// Allowed font sizes in points, enforced by the typography dialog and file loading.
    pub const SIZE_RANGE: std::ops::RangeInclusive<f32> = 8.0..=48.0;

    /// Base-size font for widgets (text editor, preview) that cannot render scripts.
    pub fn font_id(self) -> egui::FontId {
        egui::FontId::new(self.size, egui::FontFamily::Name(self.font.key().into()))
    }

    /// Font-family name under which SVG export embeds the font.
    pub fn svg_family(self) -> &'static str {
        self.font.svg_family()
    }

    /// Padded size of the rendered label, used for placement and collision tests.
    fn text_size(self, ctx: &egui::Context, text: &str) -> egui::Vec2 {
        self.galley(ctx, text).size() + LABEL_PADDING
    }

    /// Use the same script spans and embedded font metrics for canvas and export layout.
    pub fn galley(self, ctx: &egui::Context, text: &str) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        for span in parse_label(text) {
            let (size, valign) = match span.position {
                ScriptPosition::Normal => (self.size, egui::Align::BOTTOM),
                ScriptPosition::Subscript => (self.size * SCRIPT_SCALE, egui::Align::BOTTOM),
                ScriptPosition::Superscript => (self.size * SCRIPT_SCALE, egui::Align::TOP),
            };
            job.append(
                &span.text,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::new(
                        size,
                        egui::FontFamily::Name(self.font.key().into()),
                    ),
                    color: egui::Color32::BLACK,
                    valign,
                    ..Default::default()
                },
            );
        }
        // egui caches galleys by job, so re-laying out unchanged labels each frame is cheap.
        ctx.fonts_mut(|fonts| fonts.layout_job(job))
    }
}

/// Split display scripts and escapes into spans; the solver keeps the original literal name.
///
/// Typst-like notation: `_x` / `^x` script the next character only, `_(...)` / `^(...)`
/// script a parenthesized group (nesting allowed), and `\` escapes the next character.
/// A trailing marker with nothing after it is kept literally.
pub fn parse_label(input: &str) -> Vec<LabelSpan> {
    let mut spans = Vec::new();
    let mut normal = String::new();
    let mut chars = input.char_indices().peekable();

    while let Some((_, character)) = chars.next() {
        if character == '\\' {
            normal.push(chars.next().map_or('\\', |(_, escaped)| escaped));
            continue;
        }
        let position = match character {
            '_' => ScriptPosition::Subscript,
            '^' => ScriptPosition::Superscript,
            _ => {
                normal.push(character);
                continue;
            }
        };

        let Some((_, next)) = chars.next() else {
            normal.push(character);
            break;
        };
        if !normal.is_empty() {
            spans.push(LabelSpan {
                text: std::mem::take(&mut normal),
                position: ScriptPosition::Normal,
            });
        }

        let mut script = String::new();
        if next == '(' {
            let mut depth = 1;
            for (_, grouped) in chars.by_ref() {
                match grouped {
                    '(' => {
                        depth += 1;
                        script.push(grouped);
                    }
                    ')' if depth == 1 => break,
                    ')' => {
                        depth -= 1;
                        script.push(grouped);
                    }
                    _ => script.push(grouped),
                }
            }
        } else {
            script.push(next);
        }
        spans.push(LabelSpan {
            text: script,
            position,
        });
    }

    if !normal.is_empty() {
        spans.push(LabelSpan {
            text: normal,
            position: ScriptPosition::Normal,
        });
    }
    spans
}

impl LabelFont {
    /// Every font, in the order shown in the typography dialog.
    pub const ALL: [Self; 4] = [
        Self::Inter,
        Self::SourceSerif4,
        Self::IbmPlexMono,
        Self::AtkinsonHyperlegible,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Inter => "Inter",
            Self::SourceSerif4 => "Source Serif 4",
            Self::IbmPlexMono => "IBM Plex Mono",
            Self::AtkinsonHyperlegible => "Atkinson Hyperlegible",
        }
    }

    /// Name of the egui font family registered by [`install_fonts`].
    fn key(self) -> &'static str {
        match self {
            Self::Inter => "netdraw_inter",
            Self::SourceSerif4 => "netdraw_source_serif4",
            Self::IbmPlexMono => "netdraw_ibm_plex_mono",
            Self::AtkinsonHyperlegible => "netdraw_atkinson_hyperlegible",
        }
    }

    fn svg_family(self) -> &'static str {
        match self {
            Self::Inter => "NetDraw Inter",
            Self::SourceSerif4 => "NetDraw Source Serif 4",
            Self::IbmPlexMono => "NetDraw IBM Plex Mono",
            Self::AtkinsonHyperlegible => "NetDraw Atkinson Hyperlegible",
        }
    }

    /// Return embedded font data; no platform font installation is required.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Inter => include_bytes!("../assets/fonts/Inter-Regular.ttf"),
            Self::SourceSerif4 => include_bytes!("../assets/fonts/SourceSerif4-Regular.otf"),
            Self::IbmPlexMono => include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
            Self::AtkinsonHyperlegible => {
                include_bytes!("../assets/fonts/AtkinsonHyperlegible-Regular.ttf")
            }
        }
    }

    /// `(MIME type, CSS format())` for the embedded `@font-face` source.
    pub fn svg_format(self) -> (&'static str, &'static str) {
        match self {
            Self::SourceSerif4 => ("font/otf", "opentype"),
            _ => ("font/ttf", "truetype"),
        }
    }
}

/// Register named font families while retaining egui fallback glyph coverage.
pub fn install_fonts(ctx: &egui::Context) {
    let mut definitions = egui::FontDefinitions::default();
    let fallbacks = definitions
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    for font in LabelFont::ALL {
        definitions.font_data.insert(
            font.key().to_owned(),
            Arc::new(egui::FontData::from_static(font.bytes())),
        );
        let mut family_fonts = vec![font.key().to_owned()];
        family_fonts.extend(fallbacks.iter().cloned());
        definitions
            .families
            .insert(egui::FontFamily::Name(font.key().into()), family_fonts);
    }
    ctx.set_fonts(definitions);
}

/// Measured label rectangle, in the coordinate space supplied by the caller.
#[derive(Clone, Copy)]
pub struct Placement {
    pub rect: egui::Rect,
}

/// Position-attached literal label shared by internal nodes and boundary terminals.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeLabel {
    pub position: egui::Pos2,
    pub text: String,
}

/// Placements for every label kind, in the same order as their source vectors.
pub struct LabelLayout {
    pub elements: Vec<Option<Placement>>,
    pub nodes: Vec<Option<Placement>>,
    pub terminals: Vec<Option<Placement>>,
}

impl LabelLayout {
    /// Place all labels of a drawing. Order matters: element labels claim space first,
    /// then node labels avoid them, then terminal labels avoid both. The canvas and the
    /// exporter both call this so on-screen and exported layouts are identical.
    pub fn compute(
        ctx: &egui::Context,
        elements: &[Element],
        labels: &[String],
        node_labels: &[NodeLabel],
        terminal_labels: &[NodeLabel],
        style: LabelStyle,
    ) -> Self {
        let element_placements = placements(ctx, elements, labels, style);
        let node_placements =
            node_placements(ctx, elements, node_labels, &element_placements, style);
        let mut occupied = element_placements.clone();
        occupied.extend(node_placements.iter().copied());
        let terminal_placements =
            terminal_placements(ctx, elements, terminal_labels, &occupied, style);
        Self {
            elements: element_placements,
            nodes: node_placements,
            terminals: terminal_placements,
        }
    }

    /// Every placed rectangle, regardless of label kind.
    pub fn rects(&self) -> impl Iterator<Item = egui::Rect> + '_ {
        self.elements
            .iter()
            .chain(&self.nodes)
            .chain(&self.terminals)
            .flatten()
            .map(|placement| placement.rect)
    }

    /// The same layout shifted by `offset`, e.g. from canvas to screen coordinates.
    pub fn translated(&self, offset: egui::Vec2) -> Self {
        let shift = |placements: &[Option<Placement>]| {
            placements
                .iter()
                .map(|placement| {
                    placement.map(|placement| Placement {
                        rect: placement.rect.translate(offset),
                    })
                })
                .collect()
        };
        Self {
            elements: shift(&self.elements),
            nodes: shift(&self.nodes),
            terminals: shift(&self.terminals),
        }
    }
}

/// A candidate rectangle is usable if it keeps clear of every element and every label
/// placed so far.
fn is_clear(rect: egui::Rect, elements: &[Element], placed: &[Option<Placement>]) -> bool {
    elements
        .iter()
        .all(|element| clears_element(rect, *element))
        && placed
            .iter()
            .flatten()
            .all(|other| !rect.intersects(other.rect))
}

/// Place element labels in element order; empty labels retain a `None` entry.
fn placements(
    ctx: &egui::Context,
    elements: &[Element],
    labels: &[String],
    style: LabelStyle,
) -> Vec<Option<Placement>> {
    let mut placed: Vec<Option<Placement>> = Vec::with_capacity(elements.len());

    for (index, element) in elements.iter().enumerate() {
        let label = labels.get(index).map(String::as_str).unwrap_or_default();
        if label.is_empty() {
            placed.push(None);
            continue;
        }

        let size = style.text_size(ctx, label);
        let (start, end) = element.endpoints();
        let offset = end - start;
        let direction = if offset.length_sq() > 0.0 {
            offset.normalized()
        } else {
            egui::Vec2::X
        };
        let perpendicular = egui::vec2(-direction.y, direction.x);
        let center = start + offset * 0.5;

        let mut placement = None;
        // These are perpendicular distances from the element's midpoint to the label's center.
        // Start close to the element, then move outward until the label clears the diagram.
        // Arrows start farther out because their circle is wider than the zigzag.
        let initial_distance = if matches!(element, Element::Resistor(..)) {
            26.0
        } else {
            32.0
        };
        for distance in [0.0, 18.0, 36.0, 54.0, 72.0].map(|step| initial_distance + step) {
            for side in [1.0, -1.0] {
                let rect =
                    egui::Rect::from_center_size(center + perpendicular * distance * side, size);
                if is_clear(rect, elements, &placed) {
                    placement = Some(Placement { rect });
                    break;
                }
            }
            if placement.is_some() {
                break;
            }
        }

        // A densely packed diagram may leave no completely clear candidate. Keep the label at
        // its first-choice position (closest distance, positive side) rather than hiding it.
        placed.push(placement.or_else(|| {
            Some(Placement {
                rect: egui::Rect::from_center_size(center + perpendicular * initial_distance, size),
            })
        }));
    }

    placed
}

/// Place node labels after element labels, trying nearby directions before a fallback.
fn node_placements(
    ctx: &egui::Context,
    elements: &[Element],
    node_labels: &[NodeLabel],
    occupied: &[Option<Placement>],
    style: LabelStyle,
) -> Vec<Option<Placement>> {
    let mut placed: Vec<Option<Placement>> = occupied.to_vec();
    let mut node_placements = Vec::with_capacity(node_labels.len());

    for node_label in node_labels {
        if node_label.text.is_empty() {
            node_placements.push(None);
            continue;
        }
        let size = style.text_size(ctx, &node_label.text);
        let mut placement = None;
        // These are distances from the junction endpoint to the label's center. At each
        // distance, try below, above, right, left, then the two down-right/up-left diagonals
        // before moving farther away.
        for distance in [8.0, 16.0, 28.0, 46.0, 64.0, 82.0, 100.0] {
            for offset in [
                egui::Vec2::Y,
                -egui::Vec2::Y,
                egui::Vec2::X,
                -egui::Vec2::X,
                egui::vec2(0.7, 0.7),
                egui::vec2(-0.7, -0.7),
            ] {
                let rect =
                    egui::Rect::from_center_size(node_label.position + offset * distance, size);
                if is_clear(rect, elements, &placed) {
                    placement = Some(Placement { rect });
                    break;
                }
            }
            if placement.is_some() {
                break;
            }
        }
        let placement = placement.or_else(|| {
            Some(Placement {
                rect: egui::Rect::from_center_size(
                    node_label.position + egui::Vec2::Y * 28.0,
                    size,
                ),
            })
        });
        placed.push(placement);
        node_placements.push(placement);
    }

    node_placements
}

/// Place boundary labels along an attached zigzag axis, avoiding occupied rectangles.
fn terminal_placements(
    ctx: &egui::Context,
    elements: &[Element],
    terminal_labels: &[NodeLabel],
    occupied: &[Option<Placement>],
    style: LabelStyle,
) -> Vec<Option<Placement>> {
    let mut placed: Vec<Option<Placement>> = occupied.to_vec();
    let mut terminal_placements = Vec::with_capacity(terminal_labels.len());

    for terminal_label in terminal_labels {
        if terminal_label.text.is_empty() {
            terminal_placements.push(None);
            continue;
        }

        let Some(direction) = terminal_direction(elements, terminal_label.position) else {
            terminal_placements.push(None);
            continue;
        };
        let size = style.text_size(ctx, &terminal_label.text);
        // Project half of the axis-aligned label rectangle onto the resistor direction. Adding
        // this radius means `clearance` below measures from the endpoint to the label's nearest
        // edge, rather than from the endpoint to the label's center.
        let inline_radius = direction.x.abs() * size.x * 0.5 + direction.y.abs() * size.y * 0.5;
        let candidate = |clearance| {
            egui::Rect::from_center_size(
                terminal_label.position + direction * (inline_radius + clearance),
                size,
            )
        };
        // Keep terminal labels inline with the resistor. The normal edge-to-endpoint gap is
        // 8 px; if that collides, try progressively larger gaps along the same axis.
        let rect = [8.0, 16.0, 26.0, 44.0, 62.0, 80.0]
            .into_iter()
            .map(candidate)
            .find(|rect| is_clear(*rect, elements, &placed))
            .unwrap_or_else(|| candidate(8.0));
        let placement = Some(Placement { rect });
        placed.push(placement);
        terminal_placements.push(placement);
    }

    terminal_placements
}

/// Unit vector pointing outward along the resistor that ends at `position`, i.e. away
/// from the zigzag. `None` if no resistor ends there (e.g. a stale label).
fn terminal_direction(elements: &[Element], position: egui::Pos2) -> Option<egui::Vec2> {
    elements.iter().find_map(|element| {
        let Element::Resistor(start, end) = *element else {
            return None;
        };
        let inward = if start == position {
            end - start
        } else if end == position {
            start - end
        } else {
            return None;
        };
        (inward.length_sq() > 0.0).then(|| -inward.normalized())
    })
}

/// Conservative test against the element's centre line, widened by [`ELEMENT_CLEARANCE`].
fn clears_element(rect: egui::Rect, element: Element) -> bool {
    let (start, end) = element.endpoints();
    !segment_intersects_rect(start, end, rect.expand(ELEMENT_CLEARANCE))
}

fn segment_intersects_rect(start: egui::Pos2, end: egui::Pos2, rect: egui::Rect) -> bool {
    rect.contains(start)
        || rect.contains(end)
        || [
            (rect.left_top(), rect.right_top()),
            (rect.right_top(), rect.right_bottom()),
            (rect.right_bottom(), rect.left_bottom()),
            (rect.left_bottom(), rect.left_top()),
        ]
        .into_iter()
        .any(|(edge_start, edge_end)| segments_intersect(start, end, edge_start, edge_end))
}

/// Proper segment intersection; parallel (including collinear) segments report `false`,
/// which is acceptable because `segment_intersects_rect` also tests contained endpoints.
fn segments_intersect(
    first_start: egui::Pos2,
    first_end: egui::Pos2,
    second_start: egui::Pos2,
    second_end: egui::Pos2,
) -> bool {
    let first = first_end - first_start;
    let second = second_end - second_start;
    let denominator = cross(first, second);
    if denominator.abs() < f32::EPSILON {
        return false;
    }

    let between_starts = second_start - first_start;
    let first_progress = cross(between_starts, second) / denominator;
    let second_progress = cross(between_starts, first) / denominator;
    (0.0..=1.0).contains(&first_progress) && (0.0..=1.0).contains(&second_progress)
}

/// 2D cross product (z component of the 3D cross product).
fn cross(first: egui::Vec2, second: egui::Vec2) -> f32 {
    first.x * second.y - first.y * second.x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> egui::Context {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        ctx
    }

    #[test]
    fn proportional_labels_use_glyph_widths() {
        let ctx = test_context();
        let style = LabelStyle::default();

        assert!(style.text_size(&ctx, "WWWW").x > style.text_size(&ctx, "iiii").x);
    }

    #[test]
    fn every_bundled_font_is_available() {
        let ctx = test_context();
        for font in LabelFont::ALL {
            let size = LabelStyle { font, size: 18.0 }.text_size(&ctx, "NetDraw");
            assert!(size.x > 4.0, "{} did not render", font.display_name());
            assert!(size.y > 4.0, "{} did not render", font.display_name());
        }
    }

    #[test]
    fn terminal_labels_stay_on_the_resistor_axis() {
        let ctx = test_context();
        let elements = [Element::Resistor(
            egui::pos2(0.0, 20.0),
            egui::pos2(100.0, 20.0),
        )];
        let labels = [NodeLabel {
            position: egui::pos2(0.0, 20.0),
            text: String::from("A"),
        }];

        let placement =
            terminal_placements(&ctx, &elements, &labels, &[], LabelStyle::default())[0].unwrap();

        assert_eq!(placement.rect.center().y, 20.0);
        assert!(placement.rect.right() < 0.0);
    }

    #[test]
    fn parses_typst_style_scripts_and_groups() {
        assert_eq!(
            parse_label("R_1^2 V_(out)^max"),
            [
                LabelSpan {
                    text: String::from("R"),
                    position: ScriptPosition::Normal,
                },
                LabelSpan {
                    text: String::from("1"),
                    position: ScriptPosition::Subscript,
                },
                LabelSpan {
                    text: String::from("2"),
                    position: ScriptPosition::Superscript,
                },
                LabelSpan {
                    text: String::from(" V"),
                    position: ScriptPosition::Normal,
                },
                LabelSpan {
                    text: String::from("out"),
                    position: ScriptPosition::Subscript,
                },
                LabelSpan {
                    text: String::from("m"),
                    position: ScriptPosition::Superscript,
                },
                LabelSpan {
                    text: String::from("ax"),
                    position: ScriptPosition::Normal,
                },
            ]
        );
    }

    #[test]
    fn backslash_escapes_script_markers() {
        assert_eq!(
            parse_label(r"file\_name \^ literal"),
            [LabelSpan {
                text: String::from("file_name ^ literal"),
                position: ScriptPosition::Normal,
            }]
        );
    }
}
