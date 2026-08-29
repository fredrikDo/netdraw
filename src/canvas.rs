use eframe::egui;

use crate::{
    element::{
        element_at, endpoint_at, overlapping_endpoints, snap_to_endpoint, DraggedEndpoint,
        DrawMode, Element,
    },
    renderer::{
        draw_element, draw_elements, draw_endpoint_handles, draw_resistor_junction_nodes,
        draw_unconnected_resistor_terminals,
    },
};

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    let mut canvas = Canvas::default();

    eframe::run_ui_native("Simple Drawing", options, move |ui, _frame| {
        canvas.show(ui);
    })
}

struct Canvas {
    elements: Vec<Element>,
    // Set only while a new line or resistor is being drawn.
    start: Option<egui::Pos2>,
    // Set while moving one or more connected endpoints of completed elements.
    dragged_endpoints: Vec<DraggedEndpoint>,
    draw_mode: DrawMode,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            start: None,
            dragged_endpoints: Vec::new(),
            draw_mode: DrawMode::Line,
        }
    }
}

impl Canvas {
    fn show(&mut self, ui: &mut egui::Ui) {
        self.show_toolbar(ui);

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        painter.rect_filled(response.rect, 0.0, egui::Color32::WHITE);

        self.handle_erase(&response);
        self.handle_drag(&response);

        draw_elements(&painter, &self.elements);
        draw_unconnected_resistor_terminals(&painter, &self.elements);
        draw_endpoint_handles(&painter, &self.elements, response.hover_pos());
        draw_resistor_junction_nodes(&painter, &self.elements);
        self.draw_preview(&painter, &response);
        self.finish_drag(&response);
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Draw:");
            self.select_mode(ui, DrawMode::Line, "Line");
            self.select_mode(ui, DrawMode::Resistor, "Resistance");
            ui.separator();
            self.select_mode(ui, DrawMode::Move, "Move");
            ui.separator();
            self.select_mode(ui, DrawMode::Erase, "Erase");

            // Unlike drawing modes, this performs its action immediately.
            if ui.button("Clean all").clicked() {
                self.elements.clear();
                self.start = None;
                self.dragged_endpoints.clear();
            }
        });
    }

    fn select_mode(&mut self, ui: &mut egui::Ui, mode: DrawMode, label: &str) {
        if ui.selectable_label(self.draw_mode == mode, label).clicked() {
            self.draw_mode = mode;
        }
    }

    fn handle_erase(&mut self, response: &egui::Response) {
        // In erase mode, remove the topmost element under a canvas click.
        if self.draw_mode == DrawMode::Erase && response.clicked() {
            if let Some(position) = response.interact_pointer_pos() {
                if let Some(element_index) = element_at(&self.elements, position) {
                    self.elements.remove(element_index);
                }
            }
        }
    }

    fn handle_drag(&mut self, response: &egui::Response) {
        if response.drag_started() {
            if let Some(position) = response.interact_pointer_pos() {
                match self.draw_mode {
                    DrawMode::Move => {
                        self.dragged_endpoints = endpoint_at(&self.elements, position)
                            .map(|endpoint| overlapping_endpoints(&self.elements, endpoint))
                            .unwrap_or_default();
                    }
                    DrawMode::Line | DrawMode::Resistor => {
                        self.start = Some(snap_to_endpoint(&self.elements, position, &[]));
                    }
                    DrawMode::Erase => {}
                }
            }
        }

        if let Some(position) = response.interact_pointer_pos()
            && !self.dragged_endpoints.is_empty()
        {
            // Update on every frame while the pointer is held down for smooth motion.
            let snapped_position =
                snap_to_endpoint(&self.elements, position, &self.dragged_endpoints);
            for endpoint in &self.dragged_endpoints {
                self.elements[endpoint.element_index]
                    .move_endpoint(endpoint.is_start, snapped_position);
            }
        }
    }

    fn draw_preview(&self, painter: &egui::Painter, response: &egui::Response) {
        if let (Some(start), Some(position)) = (self.start, response.interact_pointer_pos()) {
            let end = snap_to_endpoint(&self.elements, position, &[]);
            match self.draw_mode {
                DrawMode::Line => draw_element(painter, Element::Line(start, end)),
                DrawMode::Resistor => draw_element(painter, Element::Resistor(start, end)),
                DrawMode::Move | DrawMode::Erase => {}
            }
        }
    }

    fn finish_drag(&mut self, response: &egui::Response) {
        if !response.drag_stopped() {
            return;
        }

        if !std::mem::take(&mut self.dragged_endpoints).is_empty() {
            // Moving an endpoint never creates an additional element on release.
            return;
        }

        if let (Some(start), Some(position)) = (self.start, response.interact_pointer_pos()) {
            let end = snap_to_endpoint(&self.elements, position, &[]);
            match self.draw_mode {
                DrawMode::Line => self.elements.push(Element::Line(start, end)),
                DrawMode::Resistor => self.elements.push(Element::Resistor(start, end)),
                DrawMode::Move | DrawMode::Erase => {}
            }
        }

        self.start = None;
    }
}
