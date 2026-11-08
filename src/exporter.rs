//! Cropped SVG generation and clipboard rasterization.
//!
//! Export consumes drawing data, not canvas UI state, so grid lines and selection
//! handles cannot leak into output. Bounds include labels and shape decorations.

use std::fmt::Write;

use eframe::egui;

use crate::document::Drawing;
use crate::element::{
    DIAGRAM_STROKE_WIDTH, Element, JUNCTION_NODE_RADIUS, junction_positions,
    prescribed_flow_symbol, resistor_body, terminal_ticks,
};
use crate::label::{self, LabelLayout};

/// Extra space around the content so edge strokes are not clipped by the crop.
const STROKE_MARGIN: f32 = 1.0;
/// Clipboard images are rendered at this many pixels per canvas point for sharp labels.
const RASTER_SCALE: f32 = 2.0;
/// Refuse clipboard images above this pixel count instead of allocating huge buffers.
const MAX_RASTER_PIXELS: u64 = 32_000_000;

// `write!` into a `String` cannot fail, so the `unwrap()` calls below never panic.

/// How the SVG refers to the label font.
enum FontSource<'a> {
    /// Embed the bundled font as a base64 `@font-face`, for standalone SVG files.
    Embedded,
    /// Refer to a family already loaded into the renderer's font database. usvg resolves
    /// installed families rather than `@font-face` rules, and skipping the embedding also
    /// avoids base64-encoding hundreds of kilobytes that would only be discarded.
    Installed(&'a str),
}

/// Export the complete drawing with embedded font data and tightly cropped bounds.
pub fn svg(ctx: &egui::Context, drawing: &Drawing) -> String {
    svg_document(ctx, drawing, FontSource::Embedded)
}

fn svg_document(ctx: &egui::Context, drawing: &Drawing, font: FontSource) -> String {
    let style = drawing.label_style;
    let elements = &drawing.elements;
    let layout = LabelLayout::compute(
        ctx,
        elements,
        &drawing.labels,
        &drawing.node_labels,
        &drawing.terminal_labels,
        style,
    );
    let bounds = Bounds::new(elements, &layout);
    let font_family = match font {
        FontSource::Embedded => style.svg_family().to_owned(),
        FontSource::Installed(family) => format!("'{}'", escape_xml(family)),
    };

    let mut output = String::new();
    writeln!(
        output,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{0}" height="{1}" viewBox="0 0 {0} {1}">"#,
        bounds.width(),
        bounds.height(),
    )
    .unwrap();
    if let FontSource::Embedded = font {
        let (mime_type, font_format) = style.font.svg_format();
        writeln!(
            output,
            "  <style>@font-face {{ font-family: '{}'; src: url('data:{};base64,{}') format('{}'); }}</style>",
            style.svg_family(),
            mime_type,
            encode_base64(style.font.bytes()),
            font_format,
        )
        .unwrap();
    }
    writeln!(
        output,
        r#"  <rect width="100%" height="100%" fill="white"/>"#
    )
    .unwrap();
    // Shift content so the cropped bounds start at the SVG origin.
    writeln!(
        output,
        r#"  <g transform="translate({} {})">"#,
        -bounds.min.x, -bounds.min.y
    )
    .unwrap();
    writeln!(
        output,
        r#"  <g fill="none" stroke="black" stroke-linecap="round" stroke-linejoin="round">"#
    )
    .unwrap();

    // Same stacking order as the canvas: symbols, terminal ticks, junction dots.
    for element in elements {
        match *element {
            Element::Resistor(start, end) => resistor(&mut output, start, end),
            Element::PrescribedFlow(start, end) => prescribed_flow(&mut output, start, end),
        }
    }
    for [start, end] in terminal_ticks(elements) {
        line(&mut output, start, end);
    }
    for position in junction_positions(elements) {
        node(&mut output, position);
    }

    writeln!(output, "  </g>").unwrap();
    let texts = drawing
        .labels
        .iter()
        .map(String::as_str)
        .zip(&layout.elements)
        .chain(
            drawing
                .node_labels
                .iter()
                .map(|label| label.text.as_str())
                .zip(&layout.nodes),
        )
        .chain(
            drawing
                .terminal_labels
                .iter()
                .map(|label| label.text.as_str())
                .zip(&layout.terminals),
        );
    for (text, placement) in texts {
        if let Some(placement) = placement {
            svg_label(&mut output, text, *placement, style.size, &font_family);
        }
    }
    writeln!(output, "  </g>").unwrap();
    writeln!(output, "</svg>").unwrap();
    output
}

/// Rasterize the same tightly cropped, clean figure used by SVG export, at
/// [`RASTER_SCALE`] pixels per canvas point on an opaque white background.
/// Rejects images above [`MAX_RASTER_PIXELS`] before allocating the raster buffer.
pub fn image(ctx: &egui::Context, drawing: &Drawing) -> Result<resvg::tiny_skia::Pixmap, String> {
    let font = drawing.label_style.font;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_font_data(font.bytes().to_vec());
    // The database holds only this font, so its first face names the family to use.
    let family = options
        .fontdb
        .faces()
        .next()
        .and_then(|face| face.families.first())
        .map(|(name, _)| name.clone())
        .ok_or("Could not load the drawing font.")?;
    let document = svg_document(ctx, drawing, FontSource::Installed(&family));
    let tree = resvg::usvg::Tree::from_str(&document, &options)
        .map_err(|error| format!("Could not render drawing: {error}"))?;
    let width = (tree.size().width() * RASTER_SCALE).ceil() as u32;
    let height = (tree.size().height() * RASTER_SCALE).ceil() as u32;
    if u64::from(width) * u64::from(height) > MAX_RASTER_PIXELS {
        return Err("Drawing is too large to copy as an image.".into());
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or("Could not allocate the drawing image.")?;
    // Rounding the size up leaves fractional edge pixels outside the SVG; fill them white too.
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(RASTER_SCALE, RASTER_SCALE),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// Geometry and text extents, including stroke margins and terminal decorations.
struct Bounds {
    min: egui::Pos2,
    max: egui::Pos2,
}

impl Bounds {
    /// Bounds of everything `svg_document` draws. An empty drawing gets a 1×1 box.
    fn new(elements: &[Element], layout: &LabelLayout) -> Self {
        let mut bounds = Self {
            min: egui::pos2(f32::INFINITY, f32::INFINITY),
            max: egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY),
        };

        for element in elements {
            match *element {
                Element::Resistor(start, end) => {
                    // Mirrors `resistor`: nothing is drawn when the body is too short.
                    if let Some(points) = resistor_body(start, end) {
                        bounds.include(start);
                        bounds.include(end);
                        points.into_iter().for_each(|point| bounds.include(point));
                    }
                }
                Element::PrescribedFlow(start, end) => {
                    if let Some(symbol) = prescribed_flow_symbol(start, end) {
                        bounds.include(start);
                        bounds.include(end);
                        bounds.include_circle(symbol.center, symbol.radius);
                    }
                }
            }
        }
        for [start, end] in terminal_ticks(elements) {
            bounds.include(start);
            bounds.include(end);
        }
        for position in junction_positions(elements) {
            bounds.include_circle(position, JUNCTION_NODE_RADIUS);
        }
        for rect in layout.rects() {
            bounds.include(rect.min);
            bounds.include(rect.max);
        }

        if !bounds.min.x.is_finite() {
            return Self {
                min: egui::Pos2::ZERO,
                max: egui::pos2(1.0, 1.0),
            };
        }

        bounds.min -= egui::Vec2::splat(STROKE_MARGIN);
        bounds.max += egui::Vec2::splat(STROKE_MARGIN);
        bounds
    }

    fn include(&mut self, position: egui::Pos2) {
        self.min = self.min.min(position);
        self.max = self.max.max(position);
    }

    fn include_circle(&mut self, center: egui::Pos2, radius: f32) {
        self.include(center - egui::Vec2::splat(radius));
        self.include(center + egui::Vec2::splat(radius));
    }

    fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    fn height(&self) -> f32 {
        self.max.y - self.min.y
    }
}

/// Escape text for use in XML character data and attribute values.
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Write one label as `<text>` with a `<tspan>` per script span. The baseline sits one
/// font size below the placement's top edge, approximating the canvas galley layout.
fn svg_label(
    output: &mut String,
    text: &str,
    placement: label::Placement,
    font_size: f32,
    font_family: &str,
) {
    write!(
        output,
        r#"  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="black">"#,
        placement.rect.left(),
        placement.rect.top() + font_size,
        font_family,
        font_size,
    )
    .unwrap();
    for span in label::parse_label(text) {
        let (font_size, baseline_shift) = match span.position {
            label::ScriptPosition::Normal => ("100%", "baseline"),
            label::ScriptPosition::Subscript => ("70%", "sub"),
            label::ScriptPosition::Superscript => ("70%", "super"),
        };
        write!(
            output,
            r#"<tspan font-size="{}" baseline-shift="{}">{}</tspan>"#,
            font_size,
            baseline_shift,
            escape_xml(&span.text),
        )
        .unwrap();
    }
    writeln!(output, "</text>").unwrap();
}

/// Standard padded base64 (RFC 4648), avoiding a dependency for one call site.
fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

fn node(output: &mut String, position: egui::Pos2) {
    writeln!(
        output,
        r#"    <circle cx="{}" cy="{}" r="{}" fill="black" stroke="none"/>"#,
        position.x, position.y, JUNCTION_NODE_RADIUS
    )
    .unwrap();
}

fn line(output: &mut String, start: egui::Pos2, end: egui::Pos2) {
    writeln!(
        output,
        r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="{}"/>"#,
        start.x, start.y, end.x, end.y, DIAGRAM_STROKE_WIDTH
    )
    .unwrap();
}

fn resistor(output: &mut String, start: egui::Pos2, end: egui::Pos2) {
    let Some(points) = resistor_body(start, end) else {
        return;
    };
    line(output, start, points[0]);
    line(output, *points.last().unwrap(), end);
    let mut path = format!("M {} {}", points[0].x, points[0].y);
    for point in &points[1..] {
        write!(path, " L {} {}", point.x, point.y).unwrap();
    }
    writeln!(
        output,
        r#"    <path d="{}" stroke-width="{}"/>"#,
        path, DIAGRAM_STROKE_WIDTH
    )
    .unwrap();
}

fn prescribed_flow(output: &mut String, start: egui::Pos2, end: egui::Pos2) {
    let Some(symbol) = prescribed_flow_symbol(start, end) else {
        return;
    };
    for [lead_start, lead_end] in symbol.leads {
        line(output, lead_start, lead_end);
    }
    writeln!(
        output,
        r#"    <circle cx="{}" cy="{}" r="{}" fill="white" stroke-width="{}"/>"#,
        symbol.center.x, symbol.center.y, symbol.radius, DIAGRAM_STROKE_WIDTH
    )
    .unwrap();
    line(output, symbol.shaft[0], symbol.shaft[1]);
    let [tip, left, right] = symbol.head;
    writeln!(
        output,
        r#"    <polygon points="{},{} {},{} {},{}" fill="black" stroke="none"/>"#,
        tip.x, tip.y, left.x, left.y, right.x, right.y
    )
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a drawing without position labels; `labels` may be shorter than `elements`.
    fn drawing(elements: &[Element], labels: &[String], label_style: label::LabelStyle) -> Drawing {
        Drawing {
            elements: elements.to_vec(),
            labels: labels.to_vec(),
            node_labels: Vec::new(),
            terminal_labels: Vec::new(),
            label_style,
        }
    }

    fn test_context() -> egui::Context {
        let ctx = egui::Context::default();
        label::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        ctx
    }

    #[test]
    fn exports_every_element_type() {
        let elements = [
            Element::Resistor(egui::pos2(10.0, 10.0), egui::pos2(110.0, 10.0)),
            Element::PrescribedFlow(egui::pos2(20.0, 20.0), egui::pos2(120.0, 20.0)),
        ];
        let ctx = test_context();

        let document = svg(
            &ctx,
            &drawing(
                &elements,
                &[String::new(), String::new()],
                label::LabelStyle::default(),
            ),
        );

        assert!(document.starts_with("<svg"));
        assert!(document.contains("width=\"114\""));
        assert!(document.contains("height=\"38\""));
        assert!(document.contains("<path"));
        assert!(document.contains("<circle"));
        assert!(document.ends_with("</svg>\n"));
    }

    #[test]
    fn exports_escaped_labels() {
        let elements = [Element::Resistor(
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
        )];
        let ctx = test_context();
        let document = svg(
            &ctx,
            &drawing(
                &elements,
                &[String::from("R<1 & R>2")],
                label::LabelStyle::default(),
            ),
        );

        assert!(document.contains("<text"));
        assert!(document.contains("R&lt;1 &amp; R&gt;2"));
    }

    #[test]
    fn exports_selected_label_typography() {
        let elements = [Element::Resistor(
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
        )];
        let ctx = test_context();
        for font in label::LabelFont::ALL {
            let document = svg(
                &ctx,
                &drawing(
                    &elements,
                    &[String::from("R1")],
                    label::LabelStyle { font, size: 22.0 },
                ),
            );
            assert!(document.contains(&format!("font-family=\"NetDraw {}\"", font.display_name())));
            assert!(document.contains(r#"font-size="22""#));
            let (mime, format) = font.svg_format();
            assert!(document.contains(&format!("data:{mime};base64,")));
            assert!(document.contains(&format!("format('{format}')")));
        }
    }

    #[test]
    fn clipboard_image_is_cropped_and_opaque() {
        let ctx = test_context();
        let elements = [Element::Resistor(
            egui::pos2(500.0, 600.0),
            egui::pos2(600.0, 600.0),
        )];
        let image = image(&ctx, &drawing(&elements, &[], label::LabelStyle::default())).unwrap();
        assert_eq!((image.width(), image.height()), (204, 44));
        assert!(image.data().chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert!(image.data().chunks_exact(4).any(|pixel| pixel[0] < 128));
    }

    #[test]
    fn clipboard_image_renders_bundled_fonts_and_scripts() {
        let ctx = test_context();
        // A degenerate source has no symbol, so dark pixels must come from the label.
        let elements = [Element::PrescribedFlow(
            egui::pos2(100.0, 100.0),
            egui::pos2(100.0, 100.0),
        )];
        for font in label::LabelFont::ALL {
            let with_label = image(
                &ctx,
                &drawing(
                    &elements,
                    &["R_(load)^2".into()],
                    label::LabelStyle { font, size: 22.0 },
                ),
            )
            .unwrap();
            let without_label = image(
                &ctx,
                &drawing(&elements, &[], label::LabelStyle { font, size: 22.0 }),
            )
            .unwrap();
            let dark_pixels = |image: &resvg::tiny_skia::Pixmap| {
                image
                    .data()
                    .chunks_exact(4)
                    .filter(|pixel| pixel[0] < 128)
                    .count()
            };
            assert!(
                dark_pixels(&with_label) > dark_pixels(&without_label) + 50,
                "label missing for {font:?}"
            );
        }
    }

    #[test]
    fn clipboard_image_rejects_excessive_dimensions() {
        let ctx = test_context();
        let elements = [Element::Resistor(
            egui::Pos2::ZERO,
            egui::pos2(100_000.0, 100_000.0),
        )];
        assert!(image(&ctx, &drawing(&elements, &[], label::LabelStyle::default())).is_err());
    }

    #[test]
    fn base64_encoder_handles_padding() {
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
    }

    #[test]
    fn exports_subscripts_and_superscripts() {
        let elements = [Element::Resistor(
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
        )];
        let ctx = test_context();
        let document = svg(
            &ctx,
            &drawing(
                &elements,
                &[String::from("R_(load)^2")],
                label::LabelStyle::default(),
            ),
        );

        assert!(document.contains(r#"baseline-shift="sub">load</tspan>"#));
        assert!(document.contains(r#"baseline-shift="super">2</tspan>"#));
        assert!(!document.contains("R_(load)^2"));
    }

    #[test]
    fn exports_resistor_with_seven_zig_segments() {
        let elements = [Element::Resistor(
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
        )];
        let ctx = test_context();
        let document = svg(
            &ctx,
            &drawing(&elements, &[String::new()], label::LabelStyle::default()),
        );
        let resistor_path = document
            .lines()
            .find(|line| line.contains("<path"))
            .unwrap();

        assert_eq!(resistor_path.matches(" L ").count(), 7);
    }
}
