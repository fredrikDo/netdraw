use eframe::egui;

#[derive(Clone, Copy)] // ??
enum Element {
    Line(egui::Pos2, egui::Pos2),
    Resistor(egui::Pos2, egui::Pos2),
}

#[derive(Clone, Copy, PartialEq)] // ??
enum DrawMode {
    Line,
    Resistor,
}

fn draw_resistor(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
) {
    let stroke = egui::Stroke::new(1.5, egui::Color32::BLACK);

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();

    // Don't draw a resistor if the points are almost identical
    if length < 5.0 {
        return;
    }

    // Direction along the resistor
    let dir = egui::Vec2::new(dx / length, dy / length);

    // Perpendicular direction
    let perp = egui::Vec2::new(-dir.y, dir.x);

    // Length of the resistor body
    let body_length = length.min(80.0);

    // Start/end of the zig-zag body
    let body_start_distance = (length - body_length) / 2.0;
    let body_end_distance = body_start_distance + body_length;

    let body_start = start + dir * body_start_distance;
    let body_end = start + dir * body_end_distance;

    // Lead wires
    painter.line_segment(
        [start, body_start],
        stroke,
    );

    painter.line_segment(
        [body_end, end],
        stroke,
    );

    // Zig-zag
    let number_of_zigs = 8;
    let amplitude = 8.0;

    let mut points = Vec::new();

    points.push(body_start);

    for i in 1..number_of_zigs {
        let t = i as f32 / number_of_zigs as f32;

        let center = body_start + dir * (body_length * t);

        let offset = if i % 2 == 1 {
            amplitude
        } else {
            -amplitude
        };

        points.push(center + perp * offset);
    }

    points.push(body_end);

    for i in 0..points.len() - 1 {
        painter.line_segment(
            [points[i], points[i + 1]],
            stroke,
        );
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();

    let mut elements: Vec<Element> = Vec::new();
    let mut start: Option<egui::Pos2> = None;

    let mut draw_mode = DrawMode::Line;

    eframe::run_ui_native(
        "Simple Drawing",
        options,
        move |ui, _frame| {

            // ----------------------------------------
            // Drawing mode buttons
            // ----------------------------------------

            ui.horizontal(|ui| {
                ui.label("Draw:");

                if ui
                    .selectable_label(
                        draw_mode == DrawMode::Line,
                        "Line",
                    )
                    .clicked()
                {
                    draw_mode = DrawMode::Line;
                }

                if ui
                    .selectable_label(
                        draw_mode == DrawMode::Resistor,
                        "Resistance",
                    )
                    .clicked()
                {
                    draw_mode = DrawMode::Resistor;
                }
            });

            //ui.separator(); Adds horizontal line

            // ----------------------------------------
            // Create the canvas
            // ----------------------------------------

            let (response, painter) = ui.allocate_painter(
                ui.available_size(),
                egui::Sense::click_and_drag(),
            );

            // White background
            painter.rect_filled(
                response.rect,
                0.0,
                egui::Color32::WHITE,
            );

            // ----------------------------------------
            // Mouse button pressed
            // ----------------------------------------

            if response.drag_started() {
                start = response.interact_pointer_pos();
            }

            // ----------------------------------------
            // Draw existing elements
            // ----------------------------------------

            for element in &elements {
                match element {
                    Element::Line(a, b) => {
                        painter.line_segment(
                            [*a, *b],
                            egui::Stroke::new(
                                1.0,
                                egui::Color32::BLACK,
                            ),
                        );
                    }

                    Element::Resistor(a, b) => {
                        draw_resistor(
                            &painter,
                            *a,
                            *b,
                        );
                    }
                }
            }

            // ----------------------------------------
            // Draw element currently being created
            // ----------------------------------------

            if let Some(start_pos) = start {
                if let Some(current_pos) =
                    response.interact_pointer_pos()
                {
                    match draw_mode {
                        DrawMode::Line => {
                            painter.line_segment(
                                [start_pos, current_pos],
                                egui::Stroke::new(
                                    1.0,
                                    egui::Color32::BLACK,
                                ),
                            );
                        }

                        DrawMode::Resistor => {
                            draw_resistor(
                                &painter,
                                start_pos,
                                current_pos,
                            );
                        }
                    }
                }
            }

            // ----------------------------------------
            // Mouse button released
            // ----------------------------------------

            if response.drag_stopped() {
                if let (Some(start_pos), Some(end_pos)) = (
                    start,
                    response.interact_pointer_pos(),
                ) {
                    match draw_mode {
                        DrawMode::Line => {
                            elements.push(
                                Element::Line(
                                    start_pos,
                                    end_pos,
                                ),
                            );
                        }

                        DrawMode::Resistor => {
                            elements.push(
                                Element::Resistor(
                                    start_pos,
                                    end_pos,
                                ),
                            );
                        }
                    }
                }

                start = None;
            }
        },
    )
}
