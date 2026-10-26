//! egui painting for diagram shapes, labels, and editing overlays.
//!
//! Callers supply screen-space geometry. Exporting is separate, but shares shape
//! and text-layout helpers so canvas and output stay consistent.

use eframe::egui;

use crate::element::{
    DIAGRAM_STROKE_WIDTH, ENDPOINT_INTERACTION_DISTANCE, ENDPOINT_RADIUS, Element,
    JUNCTION_NODE_RADIUS, resistor_body, resistor_half_width,
};
use crate::label::{LabelStyle, Placement};

const PRESCRIBED_FLOW_RADIUS: f32 = 16.0;

pub fn draw_elements(painter: &egui::Painter, elements: &[Element]) {
    for element in elements {
        draw_element(painter, *element);
    }
}

pub fn draw_labels(
    painter: &egui::Painter,
    labels: &[String],
    placements: &[Option<Placement>],
    editing_label: Option<usize>,
    style: LabelStyle,
) {
    for (index, (label, placement)) in labels.iter().zip(placements).enumerate() {
        if label.is_empty() || editing_label == Some(index) {
            continue;
        }
        if let Some(placement) = placement {
            painter.galley(
                placement.rect.left_top(),
                style.galley(painter.ctx(), label),
                egui::Color32::BLACK,
            );
        }
    }
}

pub fn draw_element(painter: &egui::Painter, element: Element) {
    match element {
        Element::Line(start, end) => {
            painter.line_segment(
                [start, end],
                egui::Stroke::new(DIAGRAM_STROKE_WIDTH, egui::Color32::BLACK),
            );
        }
        Element::Resistor(start, end) => draw_resistor(painter, start, end),
        Element::PrescribedFlow(start, end) => draw_prescribed_flow(painter, start, end),
    }
}

// Draw small handles only for endpoints near the pointer.
pub fn draw_endpoint_handles(
    painter: &egui::Painter,
    elements: &[Element],
    pointer_position: Option<egui::Pos2>,
) {
    for element in elements {
        let (start, end) = element.endpoints();

        for point in [start, end] {
            if !pointer_position
                .is_some_and(|pointer| pointer.distance(point) <= ENDPOINT_INTERACTION_DISTANCE)
            {
                continue;
            }

            // Decide later whether the handle should be filled or not!
            // painter.circle_filled(point, ENDPOINT_RADIUS, egui::Color32::WHITE);
            painter.circle_stroke(
                point,
                ENDPOINT_RADIUS,
                egui::Stroke::new(1.0, egui::Color32::PURPLE),
            );
        }
    }
}

// Draw a junction node wherever two or more resistor endpoints share a position.
pub fn draw_resistor_junction_nodes(painter: &egui::Painter, elements: &[Element]) {
    let mut endpoint_groups: Vec<(egui::Pos2, usize)> = Vec::new();

    for element in elements {
        if let Element::Resistor(start, end) = element {
            for position in [*start, *end] {
                if let Some((_, count)) = endpoint_groups
                    .iter_mut()
                    .find(|(group_position, _)| *group_position == position)
                {
                    *count += 1;
                } else {
                    endpoint_groups.push((position, 1));
                }
            }
        }
    }

    for (position, count) in endpoint_groups {
        if count >= 2 {
            painter.circle_filled(position, JUNCTION_NODE_RADIUS, egui::Color32::BLACK);
        }
    }
}

// The arrow tip is the prescribed-flow symbol's only connectable endpoint.
pub fn draw_prescribed_flow_junction_nodes(painter: &egui::Painter, elements: &[Element]) {
    for element in elements {
        if let Element::PrescribedFlow(_, end) = element {
            painter.circle_filled(*end, JUNCTION_NODE_RADIUS, egui::Color32::BLACK);
        }
    }
}

// Mark resistor endpoints that are not connected to another element with a terminal tick.
pub fn draw_unconnected_resistor_terminals(painter: &egui::Painter, elements: &[Element]) {
    let all_endpoints: Vec<_> = elements
        .iter()
        .flat_map(|element| {
            let (start, end) = element.endpoints();
            [start, end]
        })
        .collect();

    for element in elements {
        let Element::Resistor(start, end) = element else {
            continue;
        };

        let direction = *end - *start;
        let length = direction.length();
        if length == 0.0 {
            continue;
        }

        let perpendicular = egui::Vec2::new(-direction.y, direction.x) / length;
        let half_length = resistor_half_width(length);

        for position in [*start, *end] {
            let connection_count = all_endpoints
                .iter()
                .filter(|other| **other == position)
                .count();
            if connection_count == 1 {
                painter.line_segment(
                    [
                        position - perpendicular * half_length,
                        position + perpendicular * half_length,
                    ],
                    egui::Stroke::new(DIAGRAM_STROKE_WIDTH, egui::Color32::BLACK),
                );
            }
        }
    }
}

pub fn draw_resistor(painter: &egui::Painter, start: egui::Pos2, end: egui::Pos2) {
    let Some(points) = resistor_body(start, end) else {
        return;
    };
    let stroke = egui::Stroke::new(DIAGRAM_STROKE_WIDTH, egui::Color32::BLACK);
    painter.line_segment([start, points[0]], stroke);
    painter.line_segment([*points.last().unwrap(), end], stroke);
    painter.add(egui::Shape::line(points, stroke));
}

// Draw an IEEE-style independent current source: lead wires, a centered circle, and an arrow.
fn draw_prescribed_flow(painter: &egui::Painter, start: egui::Pos2, end: egui::Pos2) {
    let stroke = egui::Stroke::new(DIAGRAM_STROKE_WIDTH, egui::Color32::BLACK);
    let offset = end - start;
    let length = offset.length();

    if length == 0.0 {
        return;
    }

    let direction = offset / length;
    let center = start + offset * 0.5;
    let radius = PRESCRIBED_FLOW_RADIUS.min((length / 2.0).max(0.0));
    let circle_start = center - direction * radius;
    let circle_end = center + direction * radius;

    painter.line_segment([start, circle_start], stroke);
    painter.line_segment([circle_end, end], stroke);
    painter.circle_filled(center, radius, egui::Color32::WHITE);
    painter.circle_stroke(center, radius, stroke);

    let arrow_start = center - direction * (radius * 0.55);
    let arrow_tip = center + direction * (radius * 0.55);
    let perpendicular = egui::Vec2::new(-direction.y, direction.x);
    let arrowhead_length = radius * 0.4;
    let arrowhead_width = radius * 0.22;
    let arrowhead_base = arrow_tip - direction * arrowhead_length;

    painter.line_segment([arrow_start, arrowhead_base], stroke);
    painter.add(egui::Shape::convex_polygon(
        vec![
            arrow_tip,
            arrowhead_base + perpendicular * arrowhead_width,
            arrowhead_base - perpendicular * arrowhead_width,
        ],
        egui::Color32::BLACK,
        egui::Stroke::NONE,
    ));
}
