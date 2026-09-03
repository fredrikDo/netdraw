use std::fmt::Write;

use eframe::egui;

use crate::element::Element;
use crate::label;

const PRESCRIBED_FLOW_RADIUS: f32 = 16.0;
const JUNCTION_NODE_RADIUS: f32 = 3.0;
const RESISTOR_TERMINAL_LENGTH: f32 = 16.0;
const STROKE_MARGIN: f32 = 1.0;

pub fn svg(
    elements: &[Element],
    labels: &[String],
    node_labels: &[label::NodeLabel],
    terminal_labels: &[label::NodeLabel],
    label_style: label::LabelStyle,
) -> String {
    let label_placements = label::placements(elements, labels, label_style);
    let node_label_placements =
        label::node_placements(elements, node_labels, &label_placements, label_style);
    let mut occupied_placements = label_placements.clone();
    occupied_placements.extend(node_label_placements.iter().copied());
    let terminal_label_placements =
        label::node_placements(elements, terminal_labels, &occupied_placements, label_style);
    let bounds = Bounds::for_elements(
        elements,
        &label_placements,
        &node_label_placements,
        &terminal_label_placements,
    );
    let mut output = String::new();
    writeln!(
        output,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        bounds.width(), bounds.height(), bounds.width(), bounds.height()
    )
    .unwrap();
    writeln!(output, r#"  <rect width="100%" height="100%" fill="white"/>"#).unwrap();
    writeln!(
        output,
        r#"  <g transform="translate({} {})">"#,
        -bounds.min.x, -bounds.min.y
    )
    .unwrap();
    writeln!(output, r#"  <g fill="none" stroke="black" stroke-linecap="round">"#).unwrap();

    for element in elements {
        match *element {
            Element::Line(start, end) => line(&mut output, start, end, 2.0),
            Element::Resistor(start, end) => resistor(&mut output, start, end),
            Element::PrescribedFlow(start, end) => prescribed_flow(&mut output, start, end),
        }
    }

    resistor_terminal_ticks(&mut output, elements);
    junction_nodes(&mut output, elements);

    writeln!(output, "  </g>").unwrap();
    for (label, placement) in labels.iter().zip(&label_placements) {
        if let Some(placement) = placement {
            writeln!(
                output,
                r#"  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="black">{}</text>"#,
                placement.rect.left(),
                placement.rect.top() + label_style.size,
                label_style.svg_family(),
                label_style.size,
                escape_xml(label)
            )
            .unwrap();
        }
    }
    for (node_label, placement) in node_labels.iter().zip(&node_label_placements) {
        if let Some(placement) = placement {
            writeln!(
                output,
                r#"  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="black">{}</text>"#,
                placement.rect.left(),
                placement.rect.top() + label_style.size,
                label_style.svg_family(),
                label_style.size,
                escape_xml(&node_label.text)
            )
            .unwrap();
        }
    }
    for (terminal_label, placement) in terminal_labels.iter().zip(&terminal_label_placements) {
        if let Some(placement) = placement {
            writeln!(
                output,
                r#"  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="black">{}</text>"#,
                placement.rect.left(),
                placement.rect.top() + label_style.size,
                label_style.svg_family(),
                label_style.size,
                escape_xml(&terminal_label.text)
            )
            .unwrap();
        }
    }
    writeln!(output, "  </g>").unwrap();
    writeln!(output, "</svg>").unwrap();
    output
}

struct Bounds {
    min: egui::Pos2,
    max: egui::Pos2,
}

impl Bounds {
    fn for_elements(
        elements: &[Element],
        labels: &[Option<label::Placement>],
        node_labels: &[Option<label::Placement>],
        terminal_labels: &[Option<label::Placement>],
    ) -> Self {
        let mut bounds = Self {
            min: egui::pos2(f32::INFINITY, f32::INFINITY),
            max: egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY),
        };

        for element in elements {
            match *element {
                Element::Line(start, end) => {
                    bounds.include(start);
                    bounds.include(end);
                }
                Element::Resistor(start, end) => bounds.include_resistor(start, end),
                Element::PrescribedFlow(start, end) => bounds.include_prescribed_flow(start, end),
            }
        }
        bounds.include_resistor_terminal_ticks(elements);
        bounds.include_junction_nodes(elements);
        for placement in labels.iter().flatten() {
            bounds.include(placement.rect.min);
            bounds.include(placement.rect.max);
        }
        for placement in node_labels.iter().flatten() {
            bounds.include(placement.rect.min);
            bounds.include(placement.rect.max);
        }
        for placement in terminal_labels.iter().flatten() {
            bounds.include(placement.rect.min);
            bounds.include(placement.rect.max);
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
        self.min.x = self.min.x.min(position.x);
        self.min.y = self.min.y.min(position.y);
        self.max.x = self.max.x.max(position.x);
        self.max.y = self.max.y.max(position.y);
    }

    fn include_circle(&mut self, center: egui::Pos2, radius: f32) {
        self.include(center - egui::Vec2::splat(radius));
        self.include(center + egui::Vec2::splat(radius));
    }

    fn include_resistor(&mut self, start: egui::Pos2, end: egui::Pos2) {
        let offset = end - start;
        let length = offset.length();
        if length < 5.0 {
            return;
        }

        let direction = offset / length;
        let perpendicular = egui::Vec2::new(-direction.y, direction.x);
        let body_length = length.min(80.0);
        let body_start = start + direction * ((length - body_length) / 2.0);
        let body_end = body_start + direction * body_length;
        self.include(start);
        self.include(end);
        self.include(body_start);
        self.include(body_end);
        for index in 1..8 {
            let center = body_start + direction * (body_length * index as f32 / 8.0);
            let amplitude = if index % 2 == 1 { 8.0 } else { -8.0 };
            self.include(center + perpendicular * amplitude);
        }
    }

    fn include_prescribed_flow(&mut self, start: egui::Pos2, end: egui::Pos2) {
        let offset = end - start;
        let length = offset.length();
        if length == 0.0 {
            return;
        }

        self.include(start);
        self.include(end);
        self.include_circle(start + offset * 0.5, PRESCRIBED_FLOW_RADIUS.min(length / 2.0));
    }

    fn include_resistor_terminal_ticks(&mut self, elements: &[Element]) {
        let all_endpoints: Vec<_> = elements
            .iter()
            .flat_map(|element| {
                let (start, end) = element.endpoints();
                [start, end]
            })
            .collect();

        for element in elements {
            let Element::Resistor(start, end) = *element else {
                continue;
            };
            let direction = end - start;
            let length = direction.length();
            if length == 0.0 {
                continue;
            }

            let perpendicular = egui::Vec2::new(-direction.y, direction.x) / length;
            let half_length = RESISTOR_TERMINAL_LENGTH / 2.0;
            for position in [start, end] {
                let connections = all_endpoints
                    .iter()
                    .filter(|other| **other == position)
                    .count();
                if connections == 1 {
                    self.include(position - perpendicular * half_length);
                    self.include(position + perpendicular * half_length);
                }
            }
        }
    }

    fn include_junction_nodes(&mut self, elements: &[Element]) {
        let mut resistor_endpoints: Vec<(egui::Pos2, usize)> = Vec::new();
        for element in elements {
            if let Element::Resistor(start, end) = *element {
                for position in [start, end] {
                    if let Some((_, count)) = resistor_endpoints
                        .iter_mut()
                        .find(|(existing_position, _)| *existing_position == position)
                    {
                        *count += 1;
                    } else {
                        resistor_endpoints.push((position, 1));
                    }
                }
            }
        }

        for (position, count) in resistor_endpoints {
            if count >= 2 {
                self.include_circle(position, JUNCTION_NODE_RADIUS);
            }
        }
        for element in elements {
            if let Element::PrescribedFlow(_, end) = *element {
                self.include_circle(end, JUNCTION_NODE_RADIUS);
            }
        }
    }

    fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    fn height(&self) -> f32 {
        self.max.y - self.min.y
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn resistor_terminal_ticks(output: &mut String, elements: &[Element]) {
    let all_endpoints: Vec<_> = elements
        .iter()
        .flat_map(|element| {
            let (start, end) = element.endpoints();
            [start, end]
        })
        .collect();

    for element in elements {
        let Element::Resistor(start, end) = *element else {
            continue;
        };
        let direction = end - start;
        let length = direction.length();
        if length == 0.0 {
            continue;
        }

        let perpendicular = egui::Vec2::new(-direction.y, direction.x) / length;
        let half_length = RESISTOR_TERMINAL_LENGTH / 2.0;
        for position in [start, end] {
            let connection_count = all_endpoints
                .iter()
                .filter(|other| **other == position)
                .count();
            if connection_count == 1 {
                line(
                    output,
                    position - perpendicular * half_length,
                    position + perpendicular * half_length,
                    2.0,
                );
            }
        }
    }
}

fn junction_nodes(output: &mut String, elements: &[Element]) {
    let mut resistor_endpoints: Vec<(egui::Pos2, usize)> = Vec::new();
    for element in elements {
        if let Element::Resistor(start, end) = *element {
            for position in [start, end] {
                if let Some((_, count)) = resistor_endpoints
                    .iter_mut()
                    .find(|(existing_position, _)| *existing_position == position)
                {
                    *count += 1;
                } else {
                    resistor_endpoints.push((position, 1));
                }
            }
        }
    }

    for (position, count) in resistor_endpoints {
        if count >= 2 {
            node(output, position);
        }
    }
    for element in elements {
        if let Element::PrescribedFlow(_, end) = *element {
            node(output, end);
        }
    }
}

fn node(output: &mut String, position: egui::Pos2) {
    writeln!(
        output,
        r#"    <circle cx="{}" cy="{}" r="{}" fill="black" stroke="none"/>"#,
        position.x, position.y, JUNCTION_NODE_RADIUS
    )
    .unwrap();
}

fn line(output: &mut String, start: egui::Pos2, end: egui::Pos2, width: f32) {
    writeln!(
        output,
        r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="{}"/>"#,
        start.x, start.y, end.x, end.y, width
    )
    .unwrap();
}

fn resistor(output: &mut String, start: egui::Pos2, end: egui::Pos2) {
    let offset = end - start;
    let length = offset.length();
    if length < 5.0 {
        return;
    }

    let direction = offset / length;
    let perpendicular = egui::Vec2::new(-direction.y, direction.x);
    let body_length = length.min(80.0);
    let body_start = start + direction * ((length - body_length) / 2.0);
    let body_end = body_start + direction * body_length;
    line(output, start, body_start, 1.5);
    line(output, body_end, end, 1.5);

    let mut path = format!("M {} {}", body_start.x, body_start.y);
    for index in 1..8 {
        let center = body_start + direction * (body_length * index as f32 / 8.0);
        let amplitude = if index % 2 == 1 { 8.0 } else { -8.0 };
        let point = center + perpendicular * amplitude;
        write!(path, " L {} {}", point.x, point.y).unwrap();
    }
    write!(path, " L {} {}", body_end.x, body_end.y).unwrap();
    writeln!(output, r#"    <path d="{}" stroke-width="1.5"/>"#, path).unwrap();
}

fn prescribed_flow(output: &mut String, start: egui::Pos2, end: egui::Pos2) {
    let offset = end - start;
    let length = offset.length();
    if length == 0.0 {
        return;
    }

    let direction = offset / length;
    let center = start + offset * 0.5;
    let radius = PRESCRIBED_FLOW_RADIUS.min(length / 2.0);
    line(output, start, center - direction * radius, 2.0);
    line(output, center + direction * radius, end, 2.0);
    writeln!(
        output,
        r#"    <circle cx="{}" cy="{}" r="{}" fill="white" stroke-width="2"/>"#,
        center.x, center.y, radius
    )
    .unwrap();

    let arrow_start = center - direction * (radius * 0.55);
    let arrow_tip = center + direction * (radius * 0.55);
    let perpendicular = egui::Vec2::new(-direction.y, direction.x);
    let arrowhead_base = arrow_tip - direction * (radius * 0.4);
    let arrowhead_width = radius * 0.3;
    line(output, arrow_start, arrow_tip, 2.0);
    line(
        output,
        arrow_tip,
        arrowhead_base + perpendicular * arrowhead_width,
        2.0,
    );
    line(
        output,
        arrow_tip,
        arrowhead_base - perpendicular * arrowhead_width,
        2.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_every_element_type() {
        let elements = [
            Element::Line(egui::pos2(1.0, 2.0), egui::pos2(3.0, 4.0)),
            Element::Resistor(egui::pos2(10.0, 10.0), egui::pos2(110.0, 10.0)),
            Element::PrescribedFlow(egui::pos2(20.0, 20.0), egui::pos2(120.0, 20.0)),
        ];

        let document = svg(
            &elements,
            &[String::new(), String::new(), String::new()],
            &[],
            &[],
            label::LabelStyle::default(),
        );

        assert!(document.starts_with("<svg"));
        assert!(document.contains("width=\"124\""));
        assert!(document.contains("height=\"36\""));
        assert!(document.contains("<path"));
        assert!(document.contains("<circle"));
        assert!(document.ends_with("</svg>\n"));
    }

    #[test]
    fn exports_escaped_labels() {
        let elements = [Element::Line(egui::pos2(0.0, 0.0), egui::pos2(100.0, 0.0))];
        let document = svg(
            &elements,
            &[String::from("R<1 & R>2")],
            &[],
            &[],
            label::LabelStyle::default(),
        );

        assert!(document.contains("<text"));
        assert!(document.contains("R&lt;1 &amp; R&gt;2"));
    }

    #[test]
    fn exports_selected_label_typography() {
        let elements = [Element::Line(
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
        )];
        let document = svg(
            &elements,
            &[String::from("R1")],
            &[],
            &[],
            label::LabelStyle {
                font: label::LabelFont::Monospace,
                size: 22.0,
            },
        );

        assert!(document.contains(r#"font-family="monospace""#));
        assert!(document.contains(r#"font-size="22""#));
    }
}
