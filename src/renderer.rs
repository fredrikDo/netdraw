use eframe::egui;

use crate::element::{Element, ENDPOINT_RADIUS};

pub fn draw_elements(painter: &egui::Painter, elements: &[Element]) {
    for element in elements {
        draw_element(painter, *element);
    }
}

pub fn draw_element(painter: &egui::Painter, element: Element) {
    match element {
        Element::Line(start, end) => {
            painter.line_segment([start, end], egui::Stroke::new(1.0, egui::Color32::BLACK));
        }
        Element::Resistor(start, end) => draw_resistor(painter, start, end),
    }
}

// Draw small handles to show the points that can be moved.
pub fn draw_endpoint_handles(painter: &egui::Painter, elements: &[Element]) {
    for element in elements {
        let (start, end) = element.endpoints();

        for point in [start, end] {
            painter.circle_filled(point, ENDPOINT_RADIUS, egui::Color32::WHITE);
            painter.circle_stroke(
                point,
                ENDPOINT_RADIUS,
                egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
            );
        }
    }
}

pub fn draw_resistor(painter: &egui::Painter, start: egui::Pos2, end: egui::Pos2) {
    let stroke = egui::Stroke::new(1.5, egui::Color32::BLACK);
    let offset = end - start;
    let length = offset.length();

    // Don't draw a resistor if the points are almost identical.
    if length < 5.0 {
        return;
    }

    let direction = offset / length;
    let perpendicular = egui::Vec2::new(-direction.y, direction.x);
    let body_length = length.min(80.0);
    let body_start = start + direction * ((length - body_length) / 2.0);
    let body_end = body_start + direction * body_length;

    // Lead wires connect the endpoints to the centered zig-zag body.
    painter.line_segment([start, body_start], stroke);
    painter.line_segment([body_end, end], stroke);

    let number_of_zigs = 8;
    let amplitude = 8.0;
    let mut points = vec![body_start];

    for i in 1..number_of_zigs {
        let center = body_start + direction * (body_length * i as f32 / number_of_zigs as f32);
        let amplitude = if i % 2 == 1 { amplitude } else { -amplitude };
        points.push(center + perpendicular * amplitude);
    }

    points.push(body_end);
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
}
