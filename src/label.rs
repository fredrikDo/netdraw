use eframe::egui;
use std::sync::Arc;

use crate::element::Element;

const ELEMENT_CLEARANCE: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelFont {
    Inter,
    NotoSans,
    Cantarell,
    Comfortaa,
    DejaVuSansCondensed,
    FiraMono,
    LatinModern,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelStyle {
    pub font: LabelFont,
    pub size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptPosition {
    Normal,
    Subscript,
    Superscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSpan {
    pub text: String,
    pub position: ScriptPosition,
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self {
            font: LabelFont::Inter,
            size: 18.0,
        }
    }
}

impl LabelStyle {
    pub fn font_id(self) -> egui::FontId {
        egui::FontId::new(self.size, egui::FontFamily::Name(self.font.key().into()))
    }

    pub fn svg_family(self) -> &'static str {
        self.font.svg_family()
    }

    fn text_size(self, ctx: &egui::Context, text: &str) -> egui::Vec2 {
        self.galley(ctx, text).size() + egui::vec2(4.0, 4.0)
    }

    pub fn galley(self, ctx: &egui::Context, text: &str) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        for span in parse_label(text) {
            let (size, valign) = match span.position {
                ScriptPosition::Normal => (self.size, egui::Align::BOTTOM),
                ScriptPosition::Subscript => (self.size * 0.7, egui::Align::BOTTOM),
                ScriptPosition::Superscript => (self.size * 0.7, egui::Align::TOP),
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
        ctx.fonts_mut(|fonts| fonts.layout_job(job))
    }
}

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
    pub const ALL: [Self; 7] = [
        Self::Inter,
        Self::NotoSans,
        Self::Cantarell,
        Self::Comfortaa,
        Self::DejaVuSansCondensed,
        Self::FiraMono,
        Self::LatinModern,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Inter => "Inter",
            Self::NotoSans => "Noto Sans",
            Self::Cantarell => "Cantarell",
            Self::Comfortaa => "Comfortaa",
            Self::DejaVuSansCondensed => "DejaVu Sans Condensed",
            Self::FiraMono => "Fira Mono",
            Self::LatinModern => "Latin Modern",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Inter => "netdraw_inter",
            Self::NotoSans => "netdraw_noto_sans",
            Self::Cantarell => "netdraw_cantarell",
            Self::Comfortaa => "netdraw_comfortaa",
            Self::DejaVuSansCondensed => "netdraw_dejavu_sans_condensed",
            Self::FiraMono => "netdraw_fira_mono",
            Self::LatinModern => "netdraw_latin_modern",
        }
    }

    fn svg_family(self) -> &'static str {
        match self {
            Self::Inter => "NetDraw Inter",
            Self::NotoSans => "NetDraw Noto Sans",
            Self::Cantarell => "NetDraw Cantarell",
            Self::Comfortaa => "NetDraw Comfortaa",
            Self::DejaVuSansCondensed => "NetDraw DejaVu Sans Condensed",
            Self::FiraMono => "NetDraw Fira Mono",
            Self::LatinModern => "NetDraw Latin Modern",
        }
    }

    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Inter => include_bytes!("../assets/fonts/Inter-Regular.ttf"),
            Self::NotoSans => include_bytes!("../assets/fonts/NotoSans-Regular.ttf"),
            Self::Cantarell => include_bytes!("../assets/fonts/Cantarell-Regular.otf"),
            Self::Comfortaa => include_bytes!("../assets/fonts/Comfortaa-Regular.otf"),
            Self::DejaVuSansCondensed => {
                include_bytes!("../assets/fonts/DejaVuSansCondensed.ttf")
            }
            Self::FiraMono => include_bytes!("../assets/fonts/FiraMono-Medium.ttf"),
            Self::LatinModern => include_bytes!("../assets/fonts/lmroman10-regular.otf"),
        }
    }

    pub fn svg_format(self) -> (&'static str, &'static str) {
        match self {
            Self::Cantarell | Self::Comfortaa | Self::LatinModern => ("font/otf", "opentype"),
            _ => ("font/ttf", "truetype"),
        }
    }
}

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

#[derive(Clone, Copy)]
pub struct Placement {
    pub rect: egui::Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeLabel {
    pub position: egui::Pos2,
    pub text: String,
}

pub fn placements(
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
        for distance in [32.0, 50.0, 68.0, 86.0, 104.0] {
            for side in [1.0, -1.0] {
                let rect =
                    egui::Rect::from_center_size(center + perpendicular * distance * side, size);
                if elements.iter().all(|other| clears_element(rect, *other))
                    && placed
                        .iter()
                        .flatten()
                        .all(|other| !rect.intersects(other.rect))
                {
                    placement = Some(Placement { rect });
                    break;
                }
            }
            if placement.is_some() {
                break;
            }
        }

        // A densely packed diagram may leave no completely clear candidate. Keep the label near
        // its element, choosing the least surprising side rather than hiding it.
        placed.push(placement.or_else(|| {
            Some(Placement {
                rect: egui::Rect::from_center_size(center + perpendicular * 32.0, size),
            })
        }));
    }

    placed
}

pub fn node_placements(
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
        // distance, try the cardinal and diagonal directions before moving farther away.
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
                if elements
                    .iter()
                    .all(|element| clears_element(rect, *element))
                    && placed
                        .iter()
                        .flatten()
                        .all(|other| !rect.intersects(other.rect))
                {
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

pub fn terminal_placements(
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
            .find(|rect| {
                elements
                    .iter()
                    .all(|element| clears_element(*rect, *element))
                    && placed
                        .iter()
                        .flatten()
                        .all(|other| !rect.intersects(other.rect))
            })
            .unwrap_or_else(|| candidate(8.0));
        let placement = Some(Placement { rect });
        placed.push(placement);
        terminal_placements.push(placement);
    }

    terminal_placements
}

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
