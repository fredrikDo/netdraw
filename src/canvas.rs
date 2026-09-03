use eframe::egui;
use std::path::Path;

use crate::{
    element::{
        element_at, endpoint_at, overlapping_endpoints, snap_to_endpoint, DraggedEndpoint,
        DrawMode, Element,
    },
    renderer::{
        draw_element, draw_elements, draw_endpoint_handles, draw_prescribed_flow_junction_nodes,
        draw_resistor_junction_nodes, draw_unconnected_resistor_terminals,
    },
    exporter,
};

const CANVAS_SIZE: egui::Vec2 = egui::Vec2::new(2_400.0, 1_800.0);

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    let mut canvas = Canvas::default();

    eframe::run_ui_native("Simple Drawing", options, move |ui, _frame| {
        canvas.show(ui);
    })
}

struct Canvas {
    elements: Vec<Element>,
    // Set only while a new element is being drawn.
    start: Option<egui::Pos2>,
    // Set while moving one or more connected endpoints of completed elements.
    dragged_endpoints: Vec<DraggedEndpoint>,
    draw_mode: DrawMode,
    export_dialog_open: bool,
    export_path: String,
    export_status: Option<String>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            start: None,
            dragged_endpoints: Vec::new(),
            draw_mode: DrawMode::Line,
            export_dialog_open: false,
            export_path: String::from("canvas.svg"),
            export_status: None,
        }
    }
}

impl Canvas {
    fn show(&mut self, ui: &mut egui::Ui) {
        self.show_menu_bar(ui);
        self.show_toolbar(ui);
        self.show_export_dialog(ui.ctx());

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| self.show_canvas(ui));
    }

    fn show_canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(CANVAS_SIZE, egui::Sense::click_and_drag());
        painter.rect_filled(response.rect, 0.0, egui::Color32::WHITE);
        let origin = response.rect.min.to_vec2();
        let pointer_position = response
            .interact_pointer_pos()
            .map(|position| position - origin);

        self.handle_erase(&response, pointer_position);
        self.handle_drag(&response, pointer_position);

        // Geometry is stored relative to the scrollable canvas but rendered in screen space.
        let screen_elements: Vec<_> = self
            .elements
            .iter()
            .map(|element| element.translated(origin))
            .collect();
        draw_elements(&painter, &screen_elements);
        draw_unconnected_resistor_terminals(&painter, &screen_elements);
        draw_endpoint_handles(&painter, &screen_elements, response.hover_pos());
        draw_resistor_junction_nodes(&painter, &screen_elements);
        draw_prescribed_flow_junction_nodes(&painter, &screen_elements);
        self.draw_preview(&painter, pointer_position, origin);
        self.finish_drag(&response, pointer_position);
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Export…").clicked() {
                    self.export_dialog_open = true;
                    self.export_status = None;
                    ui.close();
                }
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                ui.label("Edit actions will appear here.");
            });
            ui.menu_button("Help", |ui| {
                ui.label("Help information will appear here.");
            });
        });
    }

    fn show_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export_dialog_open {
            return;
        }

        let mut close_dialog = false;
        egui::Window::new("Export SVG")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Save canvas as:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.export_path);
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = Self::choose_export_path(&self.export_path) {
                            self.export_path = path;
                            self.export_status = None;
                        }
                    }
                });

                if let Some(status) = &self.export_status {
                    ui.label(status);
                }

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let path = self.export_path.trim();
                        if path.is_empty() {
                            self.export_status = Some(String::from("Enter a file name."));
                        } else {
                            let path = if path.ends_with(".svg") {
                                path.to_owned()
                            } else {
                                format!("{path}.svg")
                            };
                            match std::fs::write(&path, exporter::svg(&self.elements)) {
                                Ok(()) => {
                                    self.export_status = Some(format!("Saved {path}"));
                                    close_dialog = true;
                                }
                                Err(error) => {
                                    self.export_status = Some(format!("Could not save SVG: {error}"));
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            });

        if close_dialog {
            self.export_dialog_open = false;
        }
    }

    fn choose_export_path(current_path: &str) -> Option<String> {
        let path = Path::new(current_path);
        let mut dialog = rfd::FileDialog::new()
            .add_filter("SVG image", &["svg"])
            .set_file_name(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("canvas.svg"),
            );

        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            dialog = dialog.set_directory(parent);
        }

        dialog
            .save_file()
            .map(|path| path.to_string_lossy().into_owned())
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Draw:");
            self.select_mode(ui, DrawMode::Line, "Line");
            self.select_mode(ui, DrawMode::Resistor, "Resistance");
            self.select_mode(ui, DrawMode::PrescribedFlow, "Prescribed flow");
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

    fn handle_erase(&mut self, response: &egui::Response, pointer_position: Option<egui::Pos2>) {
        // In erase mode, remove the topmost element under a canvas click.
        if self.draw_mode == DrawMode::Erase && response.clicked() {
            if let Some(position) = pointer_position {
                if let Some(element_index) = element_at(&self.elements, position) {
                    self.elements.remove(element_index);
                }
            }
        }
    }

    fn handle_drag(&mut self, response: &egui::Response, pointer_position: Option<egui::Pos2>) {
        // This must use the press state rather than `drag_started()`: because the canvas also
        // accepts clicks, egui delays `drag_started()` until the pointer moves past the click
        // threshold. Record the start point on the initial press instead.
        if response.is_pointer_button_down_on() && self.start.is_none() {
            if let Some(position) = pointer_position {
                match self.draw_mode {
                    DrawMode::Line | DrawMode::Resistor => {
                        self.start = Some(snap_to_endpoint(&self.elements, position, &[]));
                    }
                    // The arrow starts at its tail, which is intentionally not a snapping end.
                    DrawMode::PrescribedFlow => self.start = Some(position),
                    DrawMode::Move | DrawMode::Erase => {}
                }
            }
        }

        if response.drag_started() {
            if let Some(position) = pointer_position {
                match self.draw_mode {
                    DrawMode::Move => {
                        self.dragged_endpoints = endpoint_at(&self.elements, position)
                            .map(|endpoint| overlapping_endpoints(&self.elements, endpoint))
                            .unwrap_or_default();
                    }
                    DrawMode::Line
                    | DrawMode::Resistor
                    | DrawMode::PrescribedFlow
                    | DrawMode::Erase => {}
                }
            }
        }

        if let Some(position) = pointer_position
            && !self.dragged_endpoints.is_empty()
        {
            // Update on every frame while the pointer is held down for smooth motion.
            let can_snap = self.dragged_endpoints.iter().all(|endpoint| {
                self.elements[endpoint.element_index].endpoint_can_snap(endpoint.is_start)
            });
            let snapped_position = can_snap
                .then(|| snap_to_endpoint(&self.elements, position, &self.dragged_endpoints))
                .unwrap_or(position);
            for endpoint in &self.dragged_endpoints {
                self.elements[endpoint.element_index]
                    .move_endpoint(endpoint.is_start, snapped_position);
            }
        }
    }

    fn draw_preview(
        &self,
        painter: &egui::Painter,
        pointer_position: Option<egui::Pos2>,
        origin: egui::Vec2,
    ) {
        if let (Some(start), Some(position)) = (self.start, pointer_position) {
            let end = snap_to_endpoint(&self.elements, position, &[]);
            match self.draw_mode {
                DrawMode::Line => draw_element(painter, Element::Line(start, end).translated(origin)),
                DrawMode::Resistor => {
                    draw_element(painter, Element::Resistor(start, end).translated(origin))
                }
                DrawMode::PrescribedFlow => {
                    draw_element(painter, Element::PrescribedFlow(start, end).translated(origin))
                }
                DrawMode::Move | DrawMode::Erase => {}
            }
        }
    }

    fn finish_drag(&mut self, response: &egui::Response, pointer_position: Option<egui::Pos2>) {
        if !response.drag_stopped() {
            return;
        }

        if !std::mem::take(&mut self.dragged_endpoints).is_empty() {
            // Moving an endpoint never creates an additional element on release.
            return;
        }

        if let (Some(start), Some(position)) = (self.start, pointer_position) {
            let end = snap_to_endpoint(&self.elements, position, &[]);
            match self.draw_mode {
                DrawMode::Line => self.elements.push(Element::Line(start, end)),
                DrawMode::Resistor => self.elements.push(Element::Resistor(start, end)),
                DrawMode::PrescribedFlow => {
                    self.elements.push(Element::PrescribedFlow(start, end))
                }
                DrawMode::Move | DrawMode::Erase => {}
            }
        }

        self.start = None;
    }
}
