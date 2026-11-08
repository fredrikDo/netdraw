//! egui painting for diagram shapes, labels, and editing overlays.
//!
//! Callers supply screen-space geometry. Exporting is separate, but shares shape
//! and text-layout helpers so canvas and output stay consistent.

use eframe::egui;

use crate::element::{
    DIAGRAM_STROKE_WIDTH, ENDPOINT_INTERACTION_DISTANCE, ENDPOINT_RADIUS, Element,
    JUNCTION_NODE_RADIUS, junction_positions, prescribed_flow_symbol, resistor_body,
    terminal_ticks,
};
use crate::label::{LabelStyle, Placement};

const DIAGRAM_STROKE: egui::Stroke = egui::Stroke {
    width: DIAGRAM_STROKE_WIDTH,
    color: egui::Color32::BLACK,
};

/// Paint the complete diagram (no editing overlays) in stacking order: element
/// symbols, then terminal ticks, then junction dots on top.
pub fn draw_diagram(painter: &egui::Painter, elements: &[Element]) {
    for element in elements {
        draw_element(painter, *element);
    }
    for tick in terminal_ticks(elements) {
        painter.line_segment(tick, DIAGRAM_STROKE);
    }
    for position in junction_positions(elements) {
        painter.circle_filled(position, JUNCTION_NODE_RADIUS, egui::Color32::BLACK);
    }
}

/// Paint placed labels. `labels` and `placements` correspond by index; the label at
/// `editing_label` is skipped because its text editor is drawn in its place.
pub fn draw_labels<'a>(
    painter: &egui::Painter,
    labels: impl IntoIterator<Item = &'a str>,
    placements: &[Option<Placement>],
    editing_label: Option<usize>,
    style: LabelStyle,
) {
    for (index, (label, placement)) in labels.into_iter().zip(placements).enumerate() {
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

/// Paint one element symbol; also used for the preview while a new element is dragged.
pub fn draw_element(painter: &egui::Painter, element: Element) {
    match element {
        Element::Resistor(start, end) => draw_resistor(painter, start, end),
        Element::PrescribedFlow(start, end) => draw_prescribed_flow(painter, start, end),
    }
}

/// Outline the handles of endpoints near the pointer, hinting what can be grabbed.
pub fn draw_endpoint_handles(
    painter: &egui::Painter,
    elements: &[Element],
    pointer_position: Option<egui::Pos2>,
) {
    let Some(pointer) = pointer_position else {
        return;
    };
    let stroke = egui::Stroke::new(1.0, egui::Color32::PURPLE);
    for element in elements {
        let (start, end) = element.endpoints();
        for point in [start, end] {
            if pointer.distance(point) <= ENDPOINT_INTERACTION_DISTANCE {
                painter.circle_stroke(point, ENDPOINT_RADIUS, stroke);
            }
        }
    }
}

fn draw_resistor(painter: &egui::Painter, start: egui::Pos2, end: egui::Pos2) {
    let Some(points) = resistor_body(start, end) else {
        return;
    };
    painter.line_segment([start, points[0]], DIAGRAM_STROKE);
    painter.line_segment([*points.last().unwrap(), end], DIAGRAM_STROKE);
    painter.add(egui::Shape::line(points, DIAGRAM_STROKE));
}

fn draw_prescribed_flow(painter: &egui::Painter, start: egui::Pos2, end: egui::Pos2) {
    let Some(symbol) = prescribed_flow_symbol(start, end) else {
        return;
    };
    for lead in symbol.leads {
        painter.line_segment(lead, DIAGRAM_STROKE);
    }
    // Fill the circle so elements drawn underneath do not show through the symbol.
    painter.circle_filled(symbol.center, symbol.radius, egui::Color32::WHITE);
    painter.circle_stroke(symbol.center, symbol.radius, DIAGRAM_STROKE);
    painter.line_segment(symbol.shaft, DIAGRAM_STROKE);
    painter.add(egui::Shape::convex_polygon(
        symbol.head.to_vec(),
        egui::Color32::BLACK,
        egui::Stroke::NONE,
    ));
}
