use eframe::egui;
use std::path::Path;

use crate::{
    element::{
        element_at, endpoint_at, junction_positions, overlapping_endpoints, snap_to_endpoint,
        unconnected_resistor_terminal_positions, DraggedEndpoint, DrawMode, Element,
    },
    renderer::{
        draw_element, draw_elements, draw_endpoint_handles, draw_labels,
        draw_prescribed_flow_junction_nodes, draw_resistor_junction_nodes,
        draw_unconnected_resistor_terminals,
    },
    exporter,
};

const CANVAS_SIZE: egui::Vec2 = egui::Vec2::new(2_400.0, 1_800.0);
const TOOL_BUTTON_SIZE: f32 = 36.0;

#[derive(Clone, Copy)]
enum ToolIcon {
    Line,
    Resistor,
    PrescribedFlow,
    Move,
    Erase,
    Typography,
    CleanAll,
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    let mut canvas = Canvas::default();

    eframe::run_ui_native("NetDraw", options, move |ui, _frame| {
        canvas.show(ui);
    })
}

struct Canvas {
    elements: Vec<Element>,
    labels: Vec<String>,
    node_labels: Vec<crate::label::NodeLabel>,
    terminal_labels: Vec<crate::label::NodeLabel>,
    // Set only while a new element is being drawn.
    start: Option<egui::Pos2>,
    // Set while moving one or more connected endpoints of completed elements.
    dragged_endpoints: Vec<DraggedEndpoint>,
    draw_mode: DrawMode,
    export_dialog_open: bool,
    export_path: String,
    export_status: Option<String>,
    label_style_dialog_open: bool,
    label_style: crate::label::LabelStyle,
    editing_label: Option<EditingLabel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EditingLabel {
    Element(usize),
    Node(usize),
    Terminal(usize),
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            labels: Vec::new(),
            node_labels: Vec::new(),
            terminal_labels: Vec::new(),
            start: None,
            dragged_endpoints: Vec::new(),
            draw_mode: DrawMode::Line,
            export_dialog_open: false,
            export_path: String::from("canvas.svg"),
            export_status: None,
            label_style_dialog_open: false,
            label_style: crate::label::LabelStyle::default(),
            editing_label: None,
        }
    }
}

impl Canvas {
    fn show(&mut self, ui: &mut egui::Ui) {
        self.show_menu_bar(ui);
        self.show_toolbar(ui);
        self.show_export_dialog(ui.ctx());
        self.show_label_style_dialog(ui.ctx());

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
        self.handle_label_click(&response, pointer_position);
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
        // An empty label still needs a temporary location while its editor is open.
        let mut layout_labels = self.labels.clone();
        if let Some(EditingLabel::Element(index)) = self.editing_label
            && layout_labels.get(index).is_some_and(String::is_empty)
        {
            layout_labels[index] = String::from(" ");
        }
        let label_placements =
            crate::label::placements(ui.ctx(), &self.elements, &layout_labels, self.label_style);
        let mut layout_node_labels = self.node_labels.clone();
        if let Some(EditingLabel::Node(index)) = self.editing_label
            && layout_node_labels.get(index).is_some_and(|label| label.text.is_empty())
        {
            layout_node_labels[index].text = String::from(" ");
        }
        let node_label_placements = crate::label::node_placements(
            ui.ctx(),
            &self.elements,
            &layout_node_labels,
            &label_placements,
            self.label_style,
        );
        let mut layout_terminal_labels = self.terminal_labels.clone();
        if let Some(EditingLabel::Terminal(index)) = self.editing_label
            && layout_terminal_labels
                .get(index)
                .is_some_and(|label| label.text.is_empty())
        {
            layout_terminal_labels[index].text = String::from(" ");
        }
        let mut occupied_label_placements = label_placements.clone();
        occupied_label_placements.extend(node_label_placements.iter().copied());
        let terminal_label_placements = crate::label::terminal_placements(
            ui.ctx(),
            &self.elements,
            &layout_terminal_labels,
            &occupied_label_placements,
            self.label_style,
        );
        let screen_label_placements: Vec<_> = label_placements
            .iter()
            .map(|placement| placement.map(|placement| crate::label::Placement {
                rect: placement.rect.translate(origin),
            }))
            .collect();
        let screen_node_label_placements: Vec<_> = node_label_placements
            .iter()
            .map(|placement| placement.map(|placement| crate::label::Placement {
                rect: placement.rect.translate(origin),
            }))
            .collect();
        let screen_terminal_label_placements: Vec<_> = terminal_label_placements
            .iter()
            .map(|placement| placement.map(|placement| crate::label::Placement {
                rect: placement.rect.translate(origin),
            }))
            .collect();
        draw_labels(
            &painter,
            &self.labels,
            &screen_label_placements,
            self.editing_label.and_then(|editing| match editing {
                EditingLabel::Element(index) => Some(index),
                EditingLabel::Node(_) => None,
                EditingLabel::Terminal(_) => None,
            }),
            self.label_style,
        );
        let node_label_text: Vec<_> = self.node_labels.iter().map(|label| label.text.clone()).collect();
        draw_labels(
            &painter,
            &node_label_text,
            &screen_node_label_placements,
            self.editing_label.and_then(|editing| match editing {
                EditingLabel::Node(index) => Some(index),
                EditingLabel::Element(_) => None,
                EditingLabel::Terminal(_) => None,
            }),
            self.label_style,
        );
        let terminal_label_text: Vec<_> = self
            .terminal_labels
            .iter()
            .map(|label| label.text.clone())
            .collect();
        draw_labels(
            &painter,
            &terminal_label_text,
            &screen_terminal_label_placements,
            self.editing_label.and_then(|editing| match editing {
                EditingLabel::Terminal(index) => Some(index),
                EditingLabel::Element(_) | EditingLabel::Node(_) => None,
            }),
            self.label_style,
        );
        self.draw_preview(&painter, pointer_position, origin);
        self.finish_drag(&response, pointer_position);
        self.show_label_editor(
            ui,
            &screen_label_placements,
            &screen_node_label_placements,
            &screen_terminal_label_placements,
        );
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
                            match std::fs::write(
                                &path,
                                exporter::svg(
                                    ctx,
                                    &self.elements,
                                    &self.labels,
                                    &self.node_labels,
                                    &self.terminal_labels,
                                    self.label_style,
                                ),
                            ) {
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

    fn show_label_style_dialog(&mut self, ctx: &egui::Context) {
        if !self.label_style_dialog_open {
            return;
        }

        let mut open = self.label_style_dialog_open;
        egui::Window::new("Label typography")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("label_style_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Font");
                        egui::ComboBox::from_id_salt("label_font")
                            .selected_text(match self.label_style.font {
                                crate::label::LabelFont::Proportional => "Proportional",
                                crate::label::LabelFont::Monospace => "Monospace",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.label_style.font,
                                    crate::label::LabelFont::Proportional,
                                    "Proportional",
                                );
                                ui.selectable_value(
                                    &mut self.label_style.font,
                                    crate::label::LabelFont::Monospace,
                                    "Monospace",
                                );
                            });
                        ui.end_row();

                        ui.label("Font size");
                        ui.add(
                            egui::DragValue::new(&mut self.label_style.size)
                                .range(8.0..=48.0)
                                .speed(0.5)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });

                ui.separator();
                ui.label(
                    egui::RichText::new("Label preview")
                        .font(self.label_style.font_id()),
                );
            });
        self.label_style_dialog_open = open;
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            //ui.label("Draw:");
            self.select_mode(ui, DrawMode::Line, ToolIcon::Line, "Line");
            self.select_mode(
                ui,
                DrawMode::Resistor,
                ToolIcon::Resistor,
                "Resistance",
            );
            self.select_mode(
                ui,
                DrawMode::PrescribedFlow,
                ToolIcon::PrescribedFlow,
                "Prescribed flow",
            );
            ui.separator();
            self.select_mode(ui, DrawMode::Move, ToolIcon::Move, "Move");
            ui.separator();
            self.select_mode(ui, DrawMode::Erase, ToolIcon::Erase, "Erase");
            // Unlike drawing modes, this performs its action immediately.
            if Self::tool_button(ui, false, ToolIcon::CleanAll, "Clean all").clicked() {
                self.elements.clear();
                self.labels.clear();
                self.node_labels.clear();
                self.terminal_labels.clear();
                self.start = None;
                self.dragged_endpoints.clear();
                self.editing_label = None;
            }
            ui.separator();
            if Self::tool_button(ui, false, ToolIcon::Typography, "Label typography").clicked() {
                self.label_style_dialog_open = true;
            }
        });
    }

    fn select_mode(
        &mut self,
        ui: &mut egui::Ui,
        mode: DrawMode,
        icon: ToolIcon,
        label: &str,
    ) {
        if Self::tool_button(ui, self.draw_mode == mode, icon, label).clicked() {
            self.draw_mode = mode;
        }
    }

    fn tool_button(
        ui: &mut egui::Ui,
        selected: bool,
        icon: ToolIcon,
        label: &str,
    ) -> egui::Response {
        let response = ui.add(
            egui::Button::new("")
                .selected(selected)
                .min_size(egui::Vec2::splat(TOOL_BUTTON_SIZE)),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                selected,
                label,
            )
        });

        let color = ui.style().interact(&response).fg_stroke.color;
        Self::paint_tool_icon(ui.painter(), response.rect.shrink(7.0), icon, color);
        response.on_hover_text(label)
    }

    fn paint_tool_icon(
        painter: &egui::Painter,
        rect: egui::Rect,
        icon: ToolIcon,
        color: egui::Color32,
    ) {
        let center = rect.center();
        let stroke = egui::Stroke::new(1.8, color);
        let left = egui::pos2(rect.left(), center.y);
        let right = egui::pos2(rect.right(), center.y);

        match icon {
            ToolIcon::Line => {
                painter.line_segment([left, right], stroke);
            }
            ToolIcon::Resistor => {
                let lead = rect.width() * 0.18;
                let body_left = left + egui::vec2(lead, 0.0);
                let body_right = right - egui::vec2(lead, 0.0);
                painter.line_segment([left, body_left], stroke);
                painter.line_segment([body_right, right], stroke);

                let amplitude = rect.height() * 0.28;
                let mut points = vec![body_left];
                for index in 1..6 {
                    let fraction = index as f32 / 6.0;
                    let y = if index % 2 == 1 { -amplitude } else { amplitude };
                    points.push(egui::pos2(
                        egui::lerp(body_left.x..=body_right.x, fraction),
                        center.y + y,
                    ));
                }
                points.push(body_right);
                painter.add(egui::Shape::line(points, stroke));
            }
            ToolIcon::PrescribedFlow => {
                let radius = rect.height() * 0.36;
                painter.line_segment([left, center - egui::vec2(radius, 0.0)], stroke);
                painter.line_segment([center + egui::vec2(radius, 0.0), right], stroke);
                painter.circle_stroke(center, radius, stroke);
                let arrow_start = center - egui::vec2(radius * 0.55, 0.0);
                let arrow_tip = center + egui::vec2(radius * 0.55, 0.0);
                painter.line_segment([arrow_start, arrow_tip], stroke);
                painter.line_segment(
                    [arrow_tip, arrow_tip + egui::vec2(-3.5, -3.0)],
                    stroke,
                );
                painter.line_segment(
                    [arrow_tip, arrow_tip + egui::vec2(-3.5, 3.0)],
                    stroke,
                );
            }
            ToolIcon::Move => {
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), center.y),
                        egui::pos2(rect.right(), center.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x, rect.top()),
                        egui::pos2(center.x, rect.bottom()),
                    ],
                    stroke,
                );
                for (tip, offset_a, offset_b) in [
                    (left, egui::vec2(4.0, -3.0), egui::vec2(4.0, 3.0)),
                    (right, egui::vec2(-4.0, -3.0), egui::vec2(-4.0, 3.0)),
                    (
                        egui::pos2(center.x, rect.top()),
                        egui::vec2(-3.0, 4.0),
                        egui::vec2(3.0, 4.0),
                    ),
                    (
                        egui::pos2(center.x, rect.bottom()),
                        egui::vec2(-3.0, -4.0),
                        egui::vec2(3.0, -4.0),
                    ),
                ] {
                    painter.line_segment([tip, tip + offset_a], stroke);
                    painter.line_segment([tip, tip + offset_b], stroke);
                }
            }
            ToolIcon::Erase => {
                let points = [
                    center + egui::vec2(-7.0, 4.0),
                    center + egui::vec2(2.0, -7.0),
                    center + egui::vec2(8.0, -2.0),
                    center + egui::vec2(-1.0, 9.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    points.to_vec(),
                    egui::Color32::TRANSPARENT,
                    stroke,
                ));
                painter.line_segment([points[0], points[3]], stroke);
            }
            ToolIcon::Typography => {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "Aa",
                    egui::FontId::proportional(15.0),
                    color,
                );
            }
            ToolIcon::CleanAll => {
                let body = egui::Rect::from_min_max(
                    center + egui::vec2(-6.0, -5.0),
                    center + egui::vec2(6.0, 8.0),
                );
                painter.rect_stroke(body, 1.0, stroke, egui::StrokeKind::Inside);
                painter.line_segment(
                    [
                        center + egui::vec2(-8.0, -8.0),
                        center + egui::vec2(8.0, -8.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        center + egui::vec2(-3.0, -10.0),
                        center + egui::vec2(3.0, -10.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        center + egui::vec2(-2.0, -2.0),
                        center + egui::vec2(-2.0, 5.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        center + egui::vec2(2.0, -2.0),
                        center + egui::vec2(2.0, 5.0),
                    ],
                    stroke,
                );
            }
        }
    }

    fn handle_erase(&mut self, response: &egui::Response, pointer_position: Option<egui::Pos2>) {
        // In erase mode, remove the topmost element under a canvas click.
        if self.draw_mode == DrawMode::Erase && response.clicked() {
            if let Some(position) = pointer_position {
                if let Some(element_index) = element_at(&self.elements, position) {
                    self.elements.remove(element_index);
                    self.labels.remove(element_index);
                    self.editing_label = self.editing_label.and_then(|editing_index| {
                        match editing_index {
                            EditingLabel::Element(index) if index == element_index => None,
                            EditingLabel::Element(index) => Some(EditingLabel::Element(
                                index - usize::from(index > element_index),
                            )),
                            EditingLabel::Node(index) => Some(EditingLabel::Node(index)),
                            EditingLabel::Terminal(index) => Some(EditingLabel::Terminal(index)),
                        }
                    });
                    let positions = junction_positions(&self.elements);
                    self.node_labels.retain(|label| positions.contains(&label.position));
                    let terminals = unconnected_resistor_terminal_positions(&self.elements);
                    self.terminal_labels
                        .retain(|label| terminals.contains(&label.position));
                }
            }
        }
    }

    fn handle_label_click(
        &mut self,
        response: &egui::Response,
        pointer_position: Option<egui::Pos2>,
    ) {
        if self.draw_mode != DrawMode::Erase && response.clicked() {
            self.editing_label = pointer_position.and_then(|position| {
                let junctions = junction_positions(&self.elements);
                let terminals = unconnected_resistor_terminal_positions(&self.elements);
                junctions
                    .iter()
                    .position(|node| node.distance(position) <= 10.0)
                    .map(|index| {
                        let node_position = junctions[index];
                        let label_index = self
                            .node_labels
                            .iter()
                            .position(|label| label.position == node_position)
                            .unwrap_or_else(|| {
                                self.node_labels.push(crate::label::NodeLabel {
                                    position: node_position,
                                    text: String::new(),
                                });
                                self.node_labels.len() - 1
                            });
                        EditingLabel::Node(label_index)
                    })
                    .or_else(|| {
                        terminals
                            .iter()
                            .position(|terminal| terminal.distance(position) <= 10.0)
                            .map(|index| {
                                let terminal_position = terminals[index];
                                let label_index = self
                                    .terminal_labels
                                    .iter()
                                    .position(|label| label.position == terminal_position)
                                    .unwrap_or_else(|| {
                                        self.terminal_labels.push(crate::label::NodeLabel {
                                            position: terminal_position,
                                            text: String::new(),
                                        });
                                        self.terminal_labels.len() - 1
                                    });
                                EditingLabel::Terminal(label_index)
                            })
                    })
                    .or_else(|| element_at(&self.elements, position).map(EditingLabel::Element))
            });
        }
    }

    fn show_label_editor(
        &mut self,
        ui: &egui::Ui,
        placements: &[Option<crate::label::Placement>],
        node_placements: &[Option<crate::label::Placement>],
        terminal_placements: &[Option<crate::label::Placement>],
    ) {
        let Some(editing) = self.editing_label else {
            return;
        };
        let (index, placement) = match editing {
            EditingLabel::Element(index) => (index, placements.get(index)),
            EditingLabel::Node(index) => (index, node_placements.get(index)),
            EditingLabel::Terminal(index) => (index, terminal_placements.get(index)),
        };
        let Some(Some(placement)) = placement else {
            self.editing_label = None;
            return;
        };

        let mut finish_editing = false;
        egui::Area::new(egui::Id::new(("label", editing)))
            .order(egui::Order::Foreground)
            .fixed_pos(placement.rect.left_top())
            .show(ui.ctx(), |ui| {
                let response = ui.add_sized(
                    placement.rect.size(),
                    egui::TextEdit::singleline(match editing {
                        EditingLabel::Element(_) => &mut self.labels[index],
                        EditingLabel::Node(_) => &mut self.node_labels[index].text,
                        EditingLabel::Terminal(_) => &mut self.terminal_labels[index].text,
                    })
                    .font(self.label_style.font_id()),
                );
                finish_editing = response.has_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if !finish_editing {
                    response.request_focus();
                }
            });
        if finish_editing {
            self.editing_label = None;
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
            let previous_positions: Vec<_> = self
                .dragged_endpoints
                .iter()
                .map(|endpoint| {
                    let (start, end) = self.elements[endpoint.element_index].endpoints();
                    if endpoint.is_start { start } else { end }
                })
                .collect();
            for endpoint in &self.dragged_endpoints {
                self.elements[endpoint.element_index]
                    .move_endpoint(endpoint.is_start, snapped_position);
            }
            for node_label in &mut self.node_labels {
                if previous_positions.contains(&node_label.position) {
                    node_label.position = snapped_position;
                }
            }
            let junctions = junction_positions(&self.elements);
            self.node_labels
                .retain(|node_label| junctions.contains(&node_label.position));
            let terminals = unconnected_resistor_terminal_positions(&self.elements);
            self.terminal_labels
                .retain(|terminal_label| terminals.contains(&terminal_label.position));
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
            // A click is not considered a drag by egui. Discard its provisional drawing start;
            // it may instead have selected an existing element for labeling.
            if response.clicked() {
                self.start = None;
            }
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
            if matches!(
                self.draw_mode,
                DrawMode::Line | DrawMode::Resistor | DrawMode::PrescribedFlow
            ) {
                self.labels.push(String::new());
            }
        }

        self.start = None;
    }
}
