//! Desktop UI and interaction state.
//!
//! Persistent geometry is copied into `Drawing` snapshots; tools, selection,
//! dialogs, and clipboard ownership belong only to this controller.

use eframe::egui;
use std::path::{Path, PathBuf};

use crate::{
    document::Drawing,
    element::{
        DraggedEndpoint, DrawMode, ENDPOINT_RADIUS, Element, element_at, endpoint_at,
        endpoints_in_rect, junction_positions, overlapping_endpoints, snap_to_endpoint,
        unconnected_resistor_terminal_positions,
    },
    exporter,
    renderer::{
        draw_element, draw_elements, draw_endpoint_handles, draw_labels,
        draw_prescribed_flow_junction_nodes, draw_resistor_junction_nodes,
        draw_unconnected_resistor_terminals,
    },
};

const CANVAS_SIZE: egui::Vec2 = egui::Vec2::new(2_400.0, 1_800.0);
const CANVAS_BACKGROUND: egui::Color32 = egui::Color32::WHITE;
const GRID_SPACING: f32 = 20.0;
const GRID_COLOR: egui::Color32 = egui::Color32::from_gray(215);
const TOOL_BUTTON_SIZE: f32 = 36.0;
const TOOL_BUTTON_BACKGROUND: egui::Color32 = egui::Color32::BLACK;
const SELECTED_TOOL_BACKGROUND: egui::Color32 = egui::Color32::from_rgb(28, 100, 190);
const SELECTED_TOOL_BORDER: egui::Color32 = egui::Color32::from_rgb(125, 195, 255);

#[derive(Clone, Copy)]
enum ToolIcon {
    Solve,
    Camera,
    Grid,
    SnapToGrid,
    Resistor,
    PrescribedFlow,
    Select,
    Label,
    Erase,
    Typography,
    CleanAll,
    AlignHorizontal,
    AlignVertical,
    EqualLengths,
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(crate::icon::app_icon())
            .with_app_id("netdraw"),
        ..Default::default()
    };
    let mut canvas = Canvas::default();

    eframe::run_ui_native("NetDraw", options, move |ui, _frame| {
        canvas.show(ui);
    })
}

/// A solve runs on a worker thread so large networks never block the UI.
/// Dropping the receiver (closing the dialog or solving again) discards a stale result.
enum SolveState {
    Running(std::sync::mpsc::Receiver<Result<String, String>>),
    Done(Result<String, String>),
}

/// UI owner. `drawing()` extracts the persistent subset for pure downstream operations.
struct Canvas {
    elements: Vec<Element>,
    labels: Vec<String>,
    node_labels: Vec<crate::label::NodeLabel>,
    terminal_labels: Vec<crate::label::NodeLabel>,
    // Set only while a new element is being drawn.
    start: Option<egui::Pos2>,
    // Set while moving one or more connected endpoints of completed elements.
    dragged_endpoints: Vec<DraggedEndpoint>,
    dragged_endpoint_origins: Vec<egui::Pos2>,
    drag_origin: Option<egui::Pos2>,
    selected_endpoints: Vec<DraggedEndpoint>,
    selection_start: Option<egui::Pos2>,
    selection_pointer_down: bool,
    selection_drag_anchor: Option<DraggedEndpoint>,
    draw_mode: DrawMode,
    grid_enabled: bool,
    export_dialog_open: bool,
    export_path: String,
    export_status: Option<String>,
    clipboard: Option<arboard::Clipboard>,
    clipboard_status: Option<String>,
    label_style_dialog_open: bool,
    label_style: crate::label::LabelStyle,
    fonts_installed: bool,
    editing_label: Option<EditingLabel>,
    current_path: Option<PathBuf>,
    clean_drawing: Drawing,
    history_drawing: Drawing,
    undo_stack: Vec<Drawing>,
    redo_stack: Vec<Drawing>,
    solve: Option<SolveState>,
    file_status: Option<String>,
    pending_file_action: Option<FileAction>,
    allow_close: bool,
}

#[derive(Clone, Copy)]
enum FileAction {
    New,
    Open,
    Quit,
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
            dragged_endpoint_origins: Vec::new(),
            drag_origin: None,
            selected_endpoints: Vec::new(),
            selection_start: None,
            selection_pointer_down: false,
            selection_drag_anchor: None,
            draw_mode: DrawMode::Select,
            grid_enabled: false,
            export_dialog_open: false,
            export_path: String::from("canvas.svg"),
            export_status: None,
            clipboard: None,
            clipboard_status: None,
            label_style_dialog_open: false,
            label_style: crate::label::LabelStyle::default(),
            fonts_installed: false,
            editing_label: None,
            current_path: None,
            clean_drawing: Drawing::empty(),
            history_drawing: Drawing::empty(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            solve: None,
            file_status: None,
            pending_file_action: None,
            allow_close: false,
        }
    }
}

impl Canvas {
    fn show(&mut self, ui: &mut egui::Ui) {
        Self::apply_button_colors(ui);
        if !self.fonts_installed {
            crate::label::install_fonts(ui.ctx());
            self.fonts_installed = true;
        }
        self.handle_edit_shortcuts(ui.ctx());
        self.handle_window_close(ui.ctx());
        self.show_menu_bar(ui);
        self.show_toolbar(ui);
        self.show_solve_dialog(ui.ctx());
        self.show_export_dialog(ui.ctx());
        self.show_label_style_dialog(ui.ctx());
        self.show_unsaved_changes_dialog(ui.ctx());

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| self.show_canvas(ui));

        // Group a drag, erase stroke, or label editing session into one action.
        if !ui.input(|input| input.pointer.any_down()) && self.editing_label.is_none() {
            self.record_edit();
        }
    }

    fn apply_button_colors(ui: &mut egui::Ui) {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for visuals in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            visuals.weak_bg_fill = egui::Color32::BLACK;
            visuals.fg_stroke.color = egui::Color32::WHITE;
        }
    }

    fn show_canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(CANVAS_SIZE, egui::Sense::click_and_drag());
        painter.rect_filled(response.rect, 0.0, CANVAS_BACKGROUND);
        if self.grid_enabled {
            Self::draw_grid(&painter, response.rect);
        }
        let origin = response.rect.min.to_vec2();
        let pointer_position = response
            .interact_pointer_pos()
            .map(|position| position - origin);

        self.handle_selection(&response, pointer_position);
        self.handle_selected_delete(ui);
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
        self.draw_selection(&painter, pointer_position, origin);
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
            && layout_node_labels
                .get(index)
                .is_some_and(|label| label.text.is_empty())
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
            .map(|placement| {
                placement.map(|placement| crate::label::Placement {
                    rect: placement.rect.translate(origin),
                })
            })
            .collect();
        let screen_node_label_placements: Vec<_> = node_label_placements
            .iter()
            .map(|placement| {
                placement.map(|placement| crate::label::Placement {
                    rect: placement.rect.translate(origin),
                })
            })
            .collect();
        let screen_terminal_label_placements: Vec<_> = terminal_label_placements
            .iter()
            .map(|placement| {
                placement.map(|placement| crate::label::Placement {
                    rect: placement.rect.translate(origin),
                })
            })
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
        let node_label_text: Vec<_> = self
            .node_labels
            .iter()
            .map(|label| label.text.clone())
            .collect();
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

    fn draw_grid(painter: &egui::Painter, rect: egui::Rect) {
        let stroke = egui::Stroke::new(1.0, GRID_COLOR);
        for column in 0..=(CANVAS_SIZE.x / GRID_SPACING) as usize {
            let x = rect.left() + column as f32 * GRID_SPACING;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                stroke,
            );
        }
        for row in 0..=(CANVAS_SIZE.y / GRID_SPACING) as usize {
            let y = rect.top() + row as f32 * GRID_SPACING;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                stroke,
            );
        }
    }

    fn nearest_grid_intersection(position: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (position.x / GRID_SPACING).round() * GRID_SPACING,
            (position.y / GRID_SPACING).round() * GRID_SPACING,
        )
    }

    fn snap_selected_to_grid(&mut self) {
        let previous_positions: Vec<_> = self
            .selected_endpoints
            .iter()
            .map(|endpoint| {
                let (start, end) = self.elements[endpoint.element_index].endpoints();
                if endpoint.is_start { start } else { end }
            })
            .collect();
        let new_positions: Vec<_> = previous_positions
            .iter()
            .copied()
            .map(Self::nearest_grid_intersection)
            .collect();
        for (endpoint, position) in self.selected_endpoints.iter().zip(&new_positions) {
            self.elements[endpoint.element_index].move_endpoint(endpoint.is_start, *position);
        }
        self.update_endpoint_labels(&previous_positions, &new_positions);
    }

    fn snap_position(&self, position: egui::Pos2, ignored: &[DraggedEndpoint]) -> egui::Pos2 {
        if self.grid_enabled {
            Self::nearest_grid_intersection(position)
        } else {
            snap_to_endpoint(&self.elements, position, ignored)
        }
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.request_file_action(FileAction::New, ui.ctx());
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    self.request_file_action(FileAction::Open, ui.ctx());
                    ui.close();
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    self.save();
                    ui.close();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as();
                    ui.close();
                }
                ui.separator();
                if ui.button("Export…").clicked() {
                    self.export_dialog_open = true;
                    self.export_status = None;
                    ui.close();
                }
                if ui.button("Quit").clicked() {
                    self.request_file_action(FileAction::Quit, ui.ctx());
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                let can_undo =
                    !self.undo_stack.is_empty() || self.drawing() != self.history_drawing;
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo").shortcut_text("Ctrl+Z"))
                    .clicked()
                {
                    self.undo();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.redo_stack.is_empty() && self.drawing() == self.history_drawing,
                        egui::Button::new("Redo").shortcut_text("Ctrl+Y"),
                    )
                    .clicked()
                {
                    self.redo();
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                ui.label("Help information will appear here.");
            });
            ui.separator();
            let name = self
                .current_path
                .as_deref()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled");
            let marker = if self.is_dirty() { " *" } else { "" };
            ui.label(format!("{name}{marker}"));
            if let Some(status) = &self.file_status {
                ui.label(status);
            }
        });
    }

    /// Snapshot only document state; exclude interaction state and calculated results.
    fn drawing(&self) -> Drawing {
        Drawing {
            elements: self.elements.clone(),
            labels: self.labels.clone(),
            node_labels: self.node_labels.clone(),
            terminal_labels: self.terminal_labels.clone(),
            label_style: self.label_style,
        }
    }

    /// Commit one completed editing gesture, cap history, and invalidate the redo branch.
    fn record_edit(&mut self) {
        let drawing = self.drawing();
        if drawing != self.history_drawing {
            self.undo_stack
                .push(std::mem::replace(&mut self.history_drawing, drawing));
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }

    fn reset_history(&mut self) {
        self.history_drawing = self.drawing();
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn undo(&mut self) {
        self.record_edit();
        if let Some(drawing) = self.undo_stack.pop() {
            self.redo_stack.push(self.drawing());
            self.history_drawing = drawing.clone();
            self.apply_drawing(drawing);
        }
    }

    fn redo(&mut self) {
        self.record_edit();
        if let Some(drawing) = self.redo_stack.pop() {
            self.undo_stack.push(self.drawing());
            self.history_drawing = drawing.clone();
            self.apply_drawing(drawing);
        }
    }

    fn handle_edit_shortcuts(&mut self, ctx: &egui::Context) {
        // Let text fields handle their own undo while they have keyboard focus.
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
            self.undo();
        } else if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Y)) {
            self.redo();
        }
    }

    fn is_dirty(&self) -> bool {
        self.drawing() != self.clean_drawing
    }

    /// Defer destructive document replacement until unsaved changes are resolved.
    fn request_file_action(&mut self, action: FileAction, ctx: &egui::Context) {
        if self.is_dirty() {
            self.pending_file_action = Some(action);
        } else {
            self.perform_file_action(action, ctx);
        }
    }

    fn perform_file_action(&mut self, action: FileAction, ctx: &egui::Context) {
        match action {
            FileAction::New => self.new_drawing(),
            FileAction::Open => self.open(),
            FileAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn handle_window_close(&mut self, ctx: &egui::Context) {
        if !self.allow_close
            && ctx.input(|input| input.viewport().close_requested())
            && self.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending_file_action.is_none() {
                self.pending_file_action = Some(FileAction::Quit);
            }
        }
    }

    fn show_unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_file_action else {
            return;
        };
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Save your changes before continuing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() && self.save() {
                        self.pending_file_action = None;
                        self.perform_file_action(action, ctx);
                    }
                    if ui.button("Discard").clicked() {
                        self.pending_file_action = None;
                        self.perform_file_action(action, ctx);
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_file_action = None;
                    }
                });
            });
    }

    fn save(&mut self) -> bool {
        match self.current_path.clone() {
            Some(path) => self.save_to(path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("NetDraw drawing", &["netdraw"])
            .set_file_name(
                self.current_path
                    .as_deref()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .unwrap_or("drawing.netdraw"),
            );
        if let Some(parent) = self.current_path.as_deref().and_then(Path::parent) {
            dialog = dialog.set_directory(parent);
        }
        let Some(mut path) = dialog.save_file() else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension("netdraw");
        }
        self.save_to(path)
    }

    /// Write a temporary sibling first; mark the document clean only after rename succeeds.
    fn save_to(&mut self, path: PathBuf) -> bool {
        let drawing = self.drawing();
        let result = drawing.to_json().and_then(|json| {
            let temporary_path = path.with_extension("netdraw.tmp");
            std::fs::write(&temporary_path, json)
                .map_err(|error| format!("Could not save drawing: {error}"))?;
            std::fs::rename(&temporary_path, &path)
                .map_err(|error| format!("Could not finish saving drawing: {error}"))
        });
        match result {
            Ok(()) => {
                self.clean_drawing = drawing;
                self.current_path = Some(path.clone());
                self.file_status = Some(format!("Saved {}", path.display()));
                true
            }
            Err(error) => {
                self.file_status = Some(error);
                false
            }
        }
    }

    fn open(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("NetDraw drawing", &["netdraw"]);
        if let Some(parent) = self.current_path.as_deref().and_then(Path::parent) {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog.pick_file() else {
            return;
        };
        let result = std::fs::read_to_string(&path)
            .map_err(|error| format!("Could not open drawing: {error}"))
            .and_then(|json| Drawing::from_json(&json));
        match result {
            Ok(drawing) => {
                self.apply_drawing(drawing.clone());
                self.reset_history();
                self.clean_drawing = drawing;
                self.current_path = Some(path.clone());
                self.file_status = Some(format!("Opened {}", path.display()));
            }
            Err(error) => self.file_status = Some(error),
        }
    }

    fn new_drawing(&mut self) {
        let drawing = Drawing::empty();
        self.apply_drawing(drawing.clone());
        self.reset_history();
        self.clean_drawing = drawing;
        self.current_path = None;
        self.file_status = Some(String::from("New drawing"));
    }

    /// Replace persistent state and clear handles/indices belonging to the previous geometry.
    fn apply_drawing(&mut self, drawing: Drawing) {
        self.elements = drawing.elements;
        self.labels = drawing.labels;
        self.node_labels = drawing.node_labels;
        self.terminal_labels = drawing.terminal_labels;
        self.label_style = drawing.label_style;
        self.start = None;
        self.dragged_endpoints.clear();
        self.dragged_endpoint_origins.clear();
        self.drag_origin = None;
        self.selected_endpoints.clear();
        self.selection_start = None;
        self.selection_pointer_down = false;
        self.selection_drag_anchor = None;
        self.editing_label = None;
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
                                    self.export_status =
                                        Some(format!("Could not save SVG: {error}"));
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

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
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
                            .selected_text(self.label_style.font.display_name())
                            .show_ui(ui, |ui| {
                                for font in crate::label::LabelFont::ALL {
                                    ui.selectable_value(
                                        &mut self.label_style.font,
                                        font,
                                        font.display_name(),
                                    );
                                }
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
                ui.label(egui::RichText::new("Label preview").font(self.label_style.font_id()));
            });
        self.label_style_dialog_open = open;
    }

    fn copy_image(&mut self, ctx: &egui::Context) {
        let result = (|| -> Result<(), String> {
            let image = exporter::image(
                ctx,
                &self.elements,
                &self.labels,
                &self.node_labels,
                &self.terminal_labels,
                self.label_style,
            )?;
            // Keep the clipboard owner alive so Linux applications can request the image.
            if self.clipboard.is_none() {
                self.clipboard = Some(
                    arboard::Clipboard::new()
                        .map_err(|error| format!("Could not open clipboard: {error}"))?,
                );
            }
            self.clipboard
                .as_mut()
                .unwrap()
                .set_image(arboard::ImageData {
                    width: image.width() as usize,
                    height: image.height() as usize,
                    bytes: std::borrow::Cow::Borrowed(image.data()),
                })
                .map_err(|error| format!("Could not copy image: {error}"))
        })();
        self.clipboard_status = Some(match result {
            Ok(()) => "Image copied".into(),
            Err(error) => error,
        });
    }

    /// Display the last explicitly requested solve; edits do not silently recompute it.
    fn show_solve_dialog(&mut self, ctx: &egui::Context) {
        if let Some(SolveState::Running(receiver)) = &self.solve {
            match receiver.try_recv() {
                Ok(result) => self.solve = Some(SolveState::Done(result)),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.solve = Some(SolveState::Done(Err(
                        "The solver stopped unexpectedly.".into()
                    )));
                }
            }
        }
        let Some(state) = &self.solve else {
            return;
        };
        let mut open = true;
        egui::Window::new("Node expression").open(&mut open).default_width(560.0).show(ctx, |ui| {
            let SolveState::Done(result) = state else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Solving…");
                });
                return;
            };
            match result {
                Ok(expression) => {
                    ui.label("Expression for the network as it was when Solve was clicked:");
                    let mut text = expression.clone();
                    ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(5));
                    if ui.button("Copy expression").clicked() { ctx.copy_text(expression.clone()); }
                    ui.label("Assumes positive, finite conductances. Flow arrows inject positive flow. Terminal labels are known pressures; other labels are literal symbols.");
                }
                Err(message) => { ui.label(message); }
            }
        });
        if !open {
            self.solve = None;
        }
    }

    /// Solve a snapshot of the drawing in the background; later edits do not affect it.
    fn start_solve(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let drawing = self.drawing();
        let selection = self.selected_endpoints.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::network_solver::solve_selected(&drawing, &selection);
            // The receiver is gone if the dialog was closed or a newer solve started.
            if sender.send(result).is_ok() {
                ctx.request_repaint();
            }
        });
        self.solve = Some(SolveState::Running(receiver));
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if Self::tool_button(ui, self.grid_enabled, ToolIcon::Grid, "Grid: show and snap")
                .clicked()
            {
                self.grid_enabled = !self.grid_enabled;
            }
            ui.add_enabled_ui(!self.selected_endpoints.is_empty(), |ui| {
                if Self::tool_button(
                    ui,
                    false,
                    ToolIcon::SnapToGrid,
                    "Snap selected endpoints to grid",
                )
                .clicked()
                {
                    self.snap_selected_to_grid();
                }
            });
            ui.separator();
            self.select_mode(ui, DrawMode::Resistor, ToolIcon::Resistor, "Conductance");
            self.select_mode(
                ui,
                DrawMode::PrescribedFlow,
                ToolIcon::PrescribedFlow,
                "Prescribed flow",
            );
            ui.separator();
            self.select_mode(ui, DrawMode::Select, ToolIcon::Select, "Select endpoints");
            ui.add_enabled_ui(self.selected_endpoints.len() >= 2, |ui| {
                if Self::tool_button(ui, false, ToolIcon::AlignHorizontal, "Align horizontally")
                    .clicked()
                {
                    self.align_selected_endpoints(true);
                }
                if Self::tool_button(ui, false, ToolIcon::AlignVertical, "Align vertically")
                    .clicked()
                {
                    self.align_selected_endpoints(false);
                }
                if Self::tool_button(
                    ui,
                    false,
                    ToolIcon::EqualLengths,
                    "Equalize connected lengths",
                )
                .clicked()
                {
                    self.equalize_selected_lengths();
                }
            });
            if Self::tool_button(ui, false, ToolIcon::Solve, "Solve selected node").clicked() {
                self.start_solve(ui.ctx());
            }
            self.select_mode(ui, DrawMode::Label, ToolIcon::Label, "Add label");
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
                self.dragged_endpoint_origins.clear();
                self.drag_origin = None;
                self.selected_endpoints.clear();
                self.selection_start = None;
                self.selection_pointer_down = false;
                self.selection_drag_anchor = None;
                self.editing_label = None;
            }
            ui.separator();
            if Self::tool_button(ui, false, ToolIcon::Typography, "Label typography").clicked() {
                self.label_style_dialog_open = true;
            }
            ui.separator();
            ui.add_enabled_ui(!self.elements.is_empty(), |ui| {
                if Self::tool_button(ui, false, ToolIcon::Camera, "Copy image")
                    .on_hover_text(
                        "Copy a tightly cropped diagram without grid or selection handles",
                    )
                    .clicked()
                {
                    self.copy_image(ui.ctx());
                }
            });
        });
        if let Some(status) = &self.clipboard_status {
            ui.label(status);
        }
    }

    fn select_mode(&mut self, ui: &mut egui::Ui, mode: DrawMode, icon: ToolIcon, label: &str) {
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
        let fill = if selected {
            SELECTED_TOOL_BACKGROUND
        } else {
            TOOL_BUTTON_BACKGROUND
        };
        let stroke = if selected {
            egui::Stroke::new(2.0, SELECTED_TOOL_BORDER)
        } else {
            egui::Stroke::NONE
        };
        let response = ui.add(
            egui::Button::new("")
                .selected(selected)
                .fill(fill)
                .stroke(stroke)
                .min_size(egui::Vec2::splat(TOOL_BUTTON_SIZE)),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
        });

        let mut painter = ui.painter().clone();
        let color = if ui.is_enabled() {
            egui::Color32::WHITE
        } else {
            // Use solid gray rather than the disabled UI's alpha multiplier.
            painter.set_opacity(1.0);
            egui::Color32::from_gray(128)
        };
        Self::paint_tool_icon(&painter, response.rect.shrink(7.0), icon, color);
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
            ToolIcon::Solve => {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "=",
                    egui::FontId::proportional(25.0),
                    color,
                );
            }
            ToolIcon::Camera => {
                let body = egui::Rect::from_min_max(
                    center + egui::vec2(-10.0, -5.0),
                    center + egui::vec2(10.0, 8.0),
                );
                painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
                painter.circle_stroke(center + egui::vec2(0.0, 1.5), 4.0, stroke);
                painter.add(egui::Shape::line(
                    vec![
                        center + egui::vec2(-5.0, -5.0),
                        center + egui::vec2(-3.0, -9.0),
                        center + egui::vec2(3.0, -9.0),
                        center + egui::vec2(5.0, -5.0),
                    ],
                    stroke,
                ));
            }
            ToolIcon::SnapToGrid => {
                painter.line_segment(
                    [center - egui::vec2(8.0, 0.0), center + egui::vec2(8.0, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [center - egui::vec2(0.0, 8.0), center + egui::vec2(0.0, 8.0)],
                    stroke,
                );
                painter.circle_filled(center, 3.5, color);
                painter.circle_stroke(rect.left_top(), 2.5, stroke);
                painter.line_segment(
                    [
                        rect.left_top() + egui::vec2(3.0, 3.0),
                        center - egui::vec2(3.0, 3.0),
                    ],
                    stroke,
                );
            }
            ToolIcon::Grid => {
                for fraction in [0.0, 0.5, 1.0] {
                    let x = rect.left() + rect.width() * fraction;
                    let y = rect.top() + rect.height() * fraction;
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        stroke,
                    );
                    painter.line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        stroke,
                    );
                }
            }
            ToolIcon::AlignHorizontal | ToolIcon::AlignVertical => {
                let horizontal = matches!(icon, ToolIcon::AlignHorizontal);
                let offset = if horizontal {
                    egui::vec2(rect.width() * 0.4, 0.0)
                } else {
                    egui::vec2(0.0, rect.height() * 0.4)
                };
                painter.line_segment([center - offset, center + offset], stroke);
                for position in [center - offset, center, center + offset] {
                    painter.circle_filled(position, 2.8, color);
                }
            }
            ToolIcon::EqualLengths => {
                for side in [-1.0, 1.0] {
                    let bar = center + egui::vec2(side * 10.0, 0.0);
                    painter.line_segment(
                        [bar - egui::vec2(0.0, 7.0), bar + egui::vec2(0.0, 7.0)],
                        stroke,
                    );
                    let tip = center + egui::vec2(side * 6.0, 0.0);
                    painter.add(egui::Shape::line(
                        vec![
                            tip + egui::vec2(-side * 3.0, -3.0),
                            tip,
                            tip + egui::vec2(-side * 3.0, 3.0),
                        ],
                        stroke,
                    ));
                }
                painter.line_segment(
                    [center - egui::vec2(6.0, 0.0), center + egui::vec2(6.0, 0.0)],
                    stroke,
                );
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
                    let y = if index % 2 == 1 {
                        -amplitude
                    } else {
                        amplitude
                    };
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
                painter.line_segment([arrow_tip, arrow_tip + egui::vec2(-3.5, -3.0)], stroke);
                painter.line_segment([arrow_tip, arrow_tip + egui::vec2(-3.5, 3.0)], stroke);
            }
            ToolIcon::Select => {
                let tip = center + egui::vec2(-8.0, -9.0);
                let points = [
                    tip,
                    tip + egui::vec2(4.0, 17.0),
                    tip + egui::vec2(7.0, 11.0),
                    tip + egui::vec2(12.0, 17.0),
                    tip + egui::vec2(15.0, 14.0),
                    tip + egui::vec2(10.0, 9.0),
                    tip + egui::vec2(17.0, 7.0),
                ];
                painter.add(egui::Shape::closed_line(points.to_vec(), stroke));
            }
            ToolIcon::Label => {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "T",
                    egui::FontId::proportional(20.0),
                    color,
                );
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

    /// Fit selected connections toward equal lengths; loops may require angle changes.
    fn equalize_selected_lengths(&mut self) {
        if self.selected_endpoints.len() < 2 {
            return;
        }
        // Treat coincident handles as one junction, including attached unselected handles.
        let mut positions = Vec::new();
        for endpoint in &self.selected_endpoints {
            let (start, end) = self.elements[endpoint.element_index].endpoints();
            let position = if endpoint.is_start { start } else { end };
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
        let mut edges = Vec::new();
        for element in &self.elements {
            let (start, end) = element.endpoints();
            if let (Some(a), Some(b)) = (
                positions.iter().position(|p| *p == start),
                positions.iter().position(|p| *p == end),
            ) {
                let edge = (a.min(b), a.max(b));
                if a != b && !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
        if edges.len() < 2 {
            return;
        }
        let mut new_positions = equal_length_positions(&positions, &edges);
        if self.grid_enabled {
            // Grid intersections take precedence over exact equality after fitting.
            for position in &mut new_positions {
                *position = self.snap_position(*position, &[]);
            }
        }
        for element in &mut self.elements {
            let (start, end) = element.endpoints();
            for (is_start, position) in [(true, start), (false, end)] {
                if let Some(index) = positions.iter().position(|p| *p == position) {
                    element.move_endpoint(is_start, new_positions[index]);
                }
            }
        }
        self.update_endpoint_labels(&positions, &new_positions);
    }

    /// Align selected handles and move their position-attached labels with them.
    fn align_selected_endpoints(&mut self, horizontal: bool) {
        if self.selected_endpoints.len() < 2 {
            return;
        }
        let previous_positions: Vec<_> = self
            .selected_endpoints
            .iter()
            .map(|endpoint| {
                let (start, end) = self.elements[endpoint.element_index].endpoints();
                if endpoint.is_start { start } else { end }
            })
            .collect();
        let center = egui::Rect::from_points(&previous_positions).center();
        let new_positions: Vec<_> = previous_positions
            .iter()
            .map(|position| {
                let aligned = if horizontal {
                    egui::pos2(position.x, center.y)
                } else {
                    egui::pos2(center.x, position.y)
                };
                if self.grid_enabled {
                    self.snap_position(aligned, &[])
                } else {
                    aligned
                }
            })
            .collect();
        for (endpoint, position) in self.selected_endpoints.iter().zip(&new_positions) {
            self.elements[endpoint.element_index].move_endpoint(endpoint.is_start, *position);
        }
        self.update_endpoint_labels(&previous_positions, &new_positions);
    }

    fn update_endpoint_labels(
        &mut self,
        previous_positions: &[egui::Pos2],
        new_positions: &[egui::Pos2],
    ) {
        for labels in [&mut self.node_labels, &mut self.terminal_labels] {
            for label in labels {
                if let Some(index) = previous_positions
                    .iter()
                    .position(|position| *position == label.position)
                {
                    label.position = new_positions[index];
                }
            }
        }
        let junctions = junction_positions(&self.elements);
        self.node_labels
            .retain(|label| junctions.contains(&label.position));
        let terminals = unconnected_resistor_terminal_positions(&self.elements);
        self.terminal_labels
            .retain(|label| terminals.contains(&label.position));
    }

    fn handle_selection(
        &mut self,
        response: &egui::Response,
        pointer_position: Option<egui::Pos2>,
    ) {
        if self.draw_mode != DrawMode::Select {
            self.selection_start = None;
            self.selection_pointer_down = false;
            self.selection_drag_anchor = None;
            return;
        }

        if response.is_pointer_button_down_on() && !self.selection_pointer_down {
            self.selection_pointer_down = true;
            self.selection_drag_anchor = pointer_position
                .and_then(|position| endpoint_at(&self.elements, position))
                .filter(|endpoint| self.selected_endpoints.contains(endpoint));
            if self.selection_drag_anchor.is_none() {
                self.selection_start = pointer_position;
                self.selected_endpoints.clear();
                self.editing_label = None;
            }
        }
        if response.drag_stopped() {
            if let (Some(start), Some(end)) = (self.selection_start.take(), pointer_position) {
                let selection = egui::Rect::from_two_pos(start, end);
                self.selected_endpoints = endpoints_in_rect(&self.elements, selection);
            }
        } else if response.clicked() {
            self.selection_start = None;
            self.selected_endpoints = pointer_position
                .and_then(|position| endpoint_at(&self.elements, position))
                .map(|endpoint| overlapping_endpoints(&self.elements, endpoint))
                .unwrap_or_default();
        }
        if !response.is_pointer_button_down_on() {
            self.selection_pointer_down = false;
            self.selection_drag_anchor = None;
        }
    }

    fn handle_selected_delete(&mut self, ui: &egui::Ui) {
        if self.selected_endpoints.is_empty()
            || self.editing_label.is_some()
            || ui.ctx().egui_wants_keyboard_input()
            || !ui.input(|input| {
                input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace)
            })
        {
            return;
        }

        self.delete_selected_elements();
    }

    fn delete_selected_elements(&mut self) {
        let mut indices: Vec<_> = self
            .selected_endpoints
            .iter()
            .map(|endpoint| endpoint.element_index)
            .collect();
        indices.sort_unstable();
        indices.dedup();
        for index in indices.into_iter().rev() {
            self.elements.remove(index);
            self.labels.remove(index);
        }
        self.selected_endpoints.clear();
        self.dragged_endpoints.clear();
        self.dragged_endpoint_origins.clear();
        self.drag_origin = None;
        let junctions = junction_positions(&self.elements);
        self.node_labels
            .retain(|label| junctions.contains(&label.position));
        let terminals = unconnected_resistor_terminal_positions(&self.elements);
        self.terminal_labels
            .retain(|label| terminals.contains(&label.position));
    }

    fn draw_selection(
        &self,
        painter: &egui::Painter,
        pointer_position: Option<egui::Pos2>,
        origin: egui::Vec2,
    ) {
        let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(40, 110, 220));
        for endpoint in &self.selected_endpoints {
            let Some(element) = self.elements.get(endpoint.element_index) else {
                continue;
            };
            let (start, end) = element.endpoints();
            let position = if endpoint.is_start { start } else { end };
            painter.circle_stroke(position + origin, ENDPOINT_RADIUS + 2.0, stroke);
        }

        if self.draw_mode == DrawMode::Select
            && let (Some(start), Some(end)) = (self.selection_start, pointer_position)
        {
            paint_dashed_rect(
                painter,
                egui::Rect::from_two_pos(start + origin, end + origin),
                stroke,
            );
        }
    }

    fn handle_erase(&mut self, response: &egui::Response, pointer_position: Option<egui::Pos2>) {
        // In erase mode, remove the topmost element under a canvas click.
        if self.draw_mode == DrawMode::Erase && response.clicked() {
            if let Some(position) = pointer_position {
                if let Some(element_index) = element_at(&self.elements, position) {
                    if self
                        .selected_endpoints
                        .iter()
                        .any(|endpoint| endpoint.element_index == element_index)
                    {
                        self.delete_selected_elements();
                        return;
                    }
                    self.selected_endpoints.clear();
                    self.elements.remove(element_index);
                    self.labels.remove(element_index);
                    self.editing_label =
                        self.editing_label
                            .and_then(|editing_index| match editing_index {
                                EditingLabel::Element(index) if index == element_index => None,
                                EditingLabel::Element(index) => Some(EditingLabel::Element(
                                    index - usize::from(index > element_index),
                                )),
                                EditingLabel::Node(index) => Some(EditingLabel::Node(index)),
                                EditingLabel::Terminal(index) => {
                                    Some(EditingLabel::Terminal(index))
                                }
                            });
                    let positions = junction_positions(&self.elements);
                    self.node_labels
                        .retain(|label| positions.contains(&label.position));
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
        if self.draw_mode == DrawMode::Label && response.clicked() {
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
                let label = match editing {
                    EditingLabel::Element(_) => &mut self.labels[index],
                    EditingLabel::Node(_) => &mut self.node_labels[index].text,
                    EditingLabel::Terminal(_) => &mut self.terminal_labels[index].text,
                };
                let response = ui
                    .horizontal(|ui| {
                        let response = ui.add_sized(
                            placement.rect.size(),
                            egui::TextEdit::singleline(&mut *label)
                                .font(self.label_style.font_id())
                                .text_color(egui::Color32::BLACK)
                                .background_color(CANVAS_BACKGROUND),
                        );
                        if ui
                            .small_button("x₂")
                            .on_hover_text("Insert subscript marker (_)")
                            .clicked()
                        {
                            insert_label_marker(ui.ctx(), response.id, label, '_');
                            response.request_focus();
                        }
                        if ui
                            .small_button("x²")
                            .on_hover_text("Insert superscript marker (^)")
                            .clicked()
                        {
                            insert_label_marker(ui.ctx(), response.id, label, '^');
                            response.request_focus();
                        }
                        response
                    })
                    .inner;
                // A single-line TextEdit surrenders focus when Enter is pressed, so check
                // `lost_focus` as well as `has_focus` on the key event's frame.
                finish_editing = (response.has_focus() || response.lost_focus())
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
                    DrawMode::Resistor => {
                        self.start = Some(self.snap_position(position, &[]));
                    }
                    // The arrow tail only snaps when the grid is enabled.
                    DrawMode::PrescribedFlow => {
                        self.start = Some(if self.grid_enabled {
                            self.snap_position(position, &[])
                        } else {
                            position
                        });
                    }
                    DrawMode::Select | DrawMode::Label | DrawMode::Erase => {}
                }
            }
        }

        if response.drag_started() {
            if let Some(position) = pointer_position {
                match self.draw_mode {
                    DrawMode::Select => {
                        if let Some(endpoint) = self.selection_drag_anchor {
                            self.dragged_endpoints = self.selected_endpoints.clone();
                            if let Some(anchor_index) = self
                                .dragged_endpoints
                                .iter()
                                .position(|selected| *selected == endpoint)
                            {
                                self.dragged_endpoints.swap(0, anchor_index);
                            }
                            self.dragged_endpoint_origins = self
                                .dragged_endpoints
                                .iter()
                                .map(|endpoint| {
                                    let (start, end) =
                                        self.elements[endpoint.element_index].endpoints();
                                    if endpoint.is_start { start } else { end }
                                })
                                .collect();
                            self.drag_origin = Some(position);
                        }
                    }
                    DrawMode::Resistor
                    | DrawMode::PrescribedFlow
                    | DrawMode::Label
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
            let pointer_delta = position - self.drag_origin.unwrap_or(position);
            let anchor_origin = self
                .dragged_endpoint_origins
                .first()
                .copied()
                .unwrap_or(position);
            let unsnapped_anchor = anchor_origin + pointer_delta;
            let target_anchor = (self.grid_enabled || can_snap)
                .then(|| self.snap_position(unsnapped_anchor, &self.dragged_endpoints))
                .unwrap_or(unsnapped_anchor);
            let delta = target_anchor - anchor_origin;
            let previous_positions: Vec<_> = self
                .dragged_endpoints
                .iter()
                .map(|endpoint| {
                    let (start, end) = self.elements[endpoint.element_index].endpoints();
                    if endpoint.is_start { start } else { end }
                })
                .collect();
            let new_positions: Vec<_> = self
                .dragged_endpoint_origins
                .iter()
                .map(|position| *position + delta)
                .collect();
            for (endpoint, new_position) in self.dragged_endpoints.iter().zip(&new_positions) {
                self.elements[endpoint.element_index]
                    .move_endpoint(endpoint.is_start, *new_position);
            }
            self.update_endpoint_labels(&previous_positions, &new_positions);
        }
    }

    fn draw_preview(
        &self,
        painter: &egui::Painter,
        pointer_position: Option<egui::Pos2>,
        origin: egui::Vec2,
    ) {
        if let (Some(start), Some(position)) = (self.start, pointer_position) {
            let end = self.snap_position(position, &[]);
            match self.draw_mode {
                DrawMode::Resistor => {
                    draw_element(painter, Element::Resistor(start, end).translated(origin))
                }
                DrawMode::PrescribedFlow => draw_element(
                    painter,
                    Element::PrescribedFlow(start, end).translated(origin),
                ),
                DrawMode::Select | DrawMode::Label | DrawMode::Erase => {}
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
            self.dragged_endpoint_origins.clear();
            self.drag_origin = None;
            // Moving an endpoint never creates an additional element on release.
            return;
        }

        if let (Some(start), Some(position)) = (self.start, pointer_position) {
            let end = self.snap_position(position, &[]);
            match self.draw_mode {
                DrawMode::Resistor => self.elements.push(Element::Resistor(start, end)),
                DrawMode::PrescribedFlow => self.elements.push(Element::PrescribedFlow(start, end)),
                DrawMode::Select | DrawMode::Label | DrawMode::Erase => {}
            }
            if matches!(
                self.draw_mode,
                DrawMode::Resistor | DrawMode::PrescribedFlow
            ) {
                self.labels.push(String::new());
            }
        }

        self.start = None;
    }
}

// Fit equal-length vectors in the original directions first, then relax the length
// constraints. Trees can retain their angles exactly; loops may need to change them.
fn equal_length_positions(positions: &[egui::Pos2], edges: &[(usize, usize)]) -> Vec<egui::Pos2> {
    let mut result = positions.to_vec();
    if edges.is_empty() {
        return result;
    }
    let target = edges
        .iter()
        .map(|&(a, b)| positions[a].distance(positions[b]))
        .sum::<f32>()
        / edges.len() as f32;
    let directions: Vec<_> = edges
        .iter()
        .map(|&(a, b)| (positions[b] - positions[a]).normalized())
        .collect();
    let mut degrees = vec![0.0_f32; positions.len()];
    for &(a, b) in edges {
        degrees[a] += 1.0;
        degrees[b] += 1.0;
    }
    let step = 0.45 / degrees.into_iter().fold(1.0_f32, f32::max);
    for _ in 0..1000 {
        let mut corrections = vec![egui::Vec2::ZERO; positions.len()];
        let mut largest = 0.0_f32;
        for (&(a, b), direction) in edges.iter().zip(&directions) {
            let error = (result[b] - result[a]) - *direction * target;
            corrections[a] += error;
            corrections[b] -= error;
        }
        for (position, correction) in result.iter_mut().zip(corrections) {
            *position += correction * step;
            largest = largest.max(correction.length() * step);
        }
        if largest < 0.0001 {
            break;
        }
    }
    // Pairwise corrections preserve the centroid of each connected component.
    // Bound the work for overconstrained graphs where exact equality is impossible.
    for _ in 0..1000 {
        let mut largest = 0.0_f32;
        for (&(a, b), original_direction) in edges.iter().zip(&directions) {
            let delta = result[b] - result[a];
            let length = delta.length();
            let direction = if length > 0.0001 {
                delta / length
            } else {
                *original_direction
            };
            let error = length - target;
            let correction = direction * (error * 0.5);
            result[a] += correction;
            result[b] -= correction;
            largest = largest.max(error.abs());
        }
        if largest < 0.001 {
            break;
        }
    }
    result
}

fn paint_dashed_rect(painter: &egui::Painter, rect: egui::Rect, stroke: egui::Stroke) {
    for (start, end) in [
        (rect.left_top(), rect.right_top()),
        (rect.right_top(), rect.right_bottom()),
        (rect.right_bottom(), rect.left_bottom()),
        (rect.left_bottom(), rect.left_top()),
    ] {
        let offset = end - start;
        let length = offset.length();
        if length == 0.0 {
            continue;
        }
        let direction = offset / length;
        let mut distance = 0.0;
        while distance < length {
            let dash_end = (distance + 6.0).min(length);
            painter.line_segment(
                [start + direction * distance, start + direction * dash_end],
                stroke,
            );
            distance += 10.0;
        }
    }
}

/// Replace a character-based UI selection using UTF-8 byte offsets, then restore the cursor.
fn insert_label_marker(ctx: &egui::Context, id: egui::Id, label: &mut String, marker: char) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let fallback = egui::text::CCursor::new(label.chars().count());
    let [start, end] = state
        .cursor
        .char_range()
        .map(|range| range.sorted_cursors())
        .unwrap_or([fallback, fallback]);
    let start_byte = byte_index_at_char(label, start.index.0);
    let end_byte = byte_index_at_char(label, end.index.0);
    label.replace_range(start_byte..end_byte, &marker.to_string());

    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(start.index.0 + 1),
        )));
    egui::TextEdit::store_state(ctx, id, state);
}

fn byte_index_at_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_snaps_to_intersections_and_restores_endpoint_snapping_when_disabled() {
        let mut canvas = Canvas::default();
        canvas.elements.push(Element::Resistor(
            egui::pos2(13.0, 17.0),
            egui::pos2(103.0, 107.0),
        ));
        let pointer = egui::pos2(14.0, 16.0);
        assert_eq!(canvas.snap_position(pointer, &[]), egui::pos2(13.0, 17.0));
        canvas.grid_enabled = true;
        assert_eq!(canvas.snap_position(pointer, &[]), egui::pos2(20.0, 20.0));
        assert_eq!(
            canvas.snap_position(egui::pos2(47.0, 52.0), &[]),
            egui::pos2(40.0, 60.0)
        );
        canvas.grid_enabled = false;
        assert_eq!(canvas.snap_position(pointer, &[]), egui::pos2(13.0, 17.0));
    }

    #[test]
    fn snap_selected_to_grid_works_with_grid_hidden_and_supports_undo() {
        let mut canvas = Canvas::default();
        canvas.elements.push(Element::Resistor(
            egui::pos2(13.0, 23.0),
            egui::pos2(87.0, 92.0),
        ));
        canvas.labels.push(String::new());
        canvas.terminal_labels.push(crate::label::NodeLabel {
            position: egui::pos2(13.0, 23.0),
            text: "A".into(),
        });
        canvas.selected_endpoints.push(DraggedEndpoint {
            element_index: 0,
            is_start: true,
        });
        canvas.reset_history();
        let before = canvas.drawing();
        canvas.snap_selected_to_grid();
        assert_eq!(
            canvas.elements[0].endpoints(),
            (egui::pos2(20.0, 20.0), egui::pos2(87.0, 92.0))
        );
        assert_eq!(canvas.terminal_labels[0].position, egui::pos2(20.0, 20.0));
        assert!(!canvas.grid_enabled);
        assert_eq!(canvas.selected_endpoints.len(), 1);
        let after = canvas.drawing();
        canvas.undo();
        assert_eq!(canvas.drawing(), before);
        canvas.redo();
        assert_eq!(canvas.drawing(), after);
    }

    #[test]
    fn grid_toggle_does_not_change_document_or_svg() {
        let mut canvas = Canvas::default();
        canvas.elements.push(Element::Resistor(
            egui::pos2(20.0, 20.0),
            egui::pos2(100.0, 80.0),
        ));
        canvas.labels.push(String::new());
        let ctx = egui::Context::default();
        let export = |canvas: &Canvas| {
            exporter::svg(
                &ctx,
                &canvas.elements,
                &canvas.labels,
                &canvas.node_labels,
                &canvas.terminal_labels,
                canvas.label_style,
            )
        };
        let drawing = canvas.drawing();
        let svg = export(&canvas);
        canvas.grid_enabled = true;
        assert_eq!(canvas.drawing(), drawing);
        assert_eq!(export(&canvas), svg);
    }

    #[test]
    fn undo_redo_restores_drawing_and_saved_state() {
        let mut canvas = Canvas::default();
        let empty = canvas.drawing();
        canvas.elements.push(Element::Resistor(
            egui::pos2(0.0, 0.0),
            egui::pos2(50.0, 0.0),
        ));
        canvas.labels.push("R_1".into());
        canvas.label_style.font = crate::label::LabelFont::SourceSerif4;
        let edited = canvas.drawing();
        canvas.clean_drawing = edited.clone();
        canvas.undo(); // Includes changes that have not yet been committed at frame end.
        assert_eq!(canvas.drawing(), empty);
        assert!(canvas.is_dirty());
        canvas.redo();
        assert_eq!(canvas.drawing(), edited);
        assert!(!canvas.is_dirty());
        canvas.undo();
        canvas.label_style.size = 30.0;
        canvas.record_edit();
        assert!(canvas.redo_stack.is_empty());
    }

    #[test]
    fn new_drawing_resets_history() {
        let mut canvas = Canvas::default();
        canvas.label_style.size = 30.0;
        canvas.record_edit();
        canvas.undo();
        canvas.new_drawing();
        canvas.redo();
        assert_eq!(canvas.drawing(), Drawing::empty());
        assert!(canvas.undo_stack.is_empty());
        assert!(canvas.redo_stack.is_empty());
    }

    #[test]
    fn control_z_and_y_restore_drawing() {
        let ctx = egui::Context::default();
        let mut canvas = Canvas::default();
        canvas.label_style.size = 30.0;
        canvas.record_edit();
        for (key, expected_size) in [(egui::Key::Z, 22.0), (egui::Key::Y, 30.0)] {
            let mut input = egui::RawInput::default();
            input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            });
            let mut output = ctx.run_ui(input, |ui| canvas.handle_edit_shortcuts(ui.ctx()));
            output.textures_delta.clear();
            assert_eq!(canvas.label_style.size, expected_size);
        }
    }

    #[test]
    fn equal_lengths_preserves_chain_angles_and_center() {
        let points = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 200.0),
        ];
        let result = equal_length_positions(&points, &[(0, 1), (1, 2)]);
        assert!((result[0].distance(result[1]) - 150.0).abs() < 0.01);
        assert!((result[1].distance(result[2]) - 150.0).abs() < 0.01);
        assert!((result[0].y - result[1].y).abs() < 0.01);
        assert!((result[1].x - result[2].x).abs() < 0.01);
        let before = points
            .iter()
            .map(|p| p.to_vec2())
            .fold(egui::Vec2::ZERO, |sum, p| sum + p);
        let after = result
            .iter()
            .map(|p| p.to_vec2())
            .fold(egui::Vec2::ZERO, |sum, p| sum + p);
        assert!((before - after).length() < 0.01);
    }

    #[test]
    fn equal_lengths_handles_closed_loops_and_degenerate_selections() {
        let points = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 200.0),
            egui::pos2(0.0, 200.0),
        ];
        let edges = [(0, 1), (1, 2), (2, 3), (3, 0)];
        let result = equal_length_positions(&points, &edges);
        for (a, b) in edges {
            assert!((result[a].distance(result[b]) - 150.0).abs() < 0.01);
        }
        assert_eq!(equal_length_positions(&points, &[]), points);
        let coincident = vec![egui::Pos2::ZERO; 3];
        assert_eq!(
            equal_length_positions(&coincident, &[(0, 1), (1, 2)]),
            coincident
        );
    }

    #[test]
    fn equal_lengths_keeps_junctions_labels_and_undo() {
        let mut canvas = Canvas::default();
        canvas.elements = vec![
            Element::Resistor(egui::pos2(0.0, 0.0), egui::pos2(100.0, 0.0)),
            Element::Resistor(egui::pos2(100.0, 0.0), egui::pos2(100.0, 200.0)),
            Element::Resistor(egui::pos2(100.0, 0.0), egui::pos2(300.0, 300.0)),
        ];
        canvas.labels = vec![String::new(); 3];
        canvas.selected_endpoints = vec![
            DraggedEndpoint {
                element_index: 0,
                is_start: true,
            },
            DraggedEndpoint {
                element_index: 0,
                is_start: false,
            },
            DraggedEndpoint {
                element_index: 1,
                is_start: false,
            },
        ];
        canvas.node_labels.push(crate::label::NodeLabel {
            position: egui::pos2(100.0, 0.0),
            text: "A".into(),
        });
        canvas.reset_history();
        let before = canvas.drawing();
        canvas.equalize_selected_lengths();
        let after = canvas.drawing();
        assert_eq!(
            canvas.elements[0].endpoints().1,
            canvas.elements[1].endpoints().0
        );
        assert_eq!(
            canvas.elements[0].endpoints().1,
            canvas.elements[2].endpoints().0
        );
        assert_eq!(
            canvas.node_labels[0].position,
            canvas.elements[0].endpoints().1
        );
        assert_eq!(canvas.elements[2].endpoints().1, egui::pos2(300.0, 300.0));
        canvas.undo();
        assert_eq!(canvas.drawing(), before);
        canvas.redo();
        assert_eq!(canvas.drawing(), after);
    }

    #[test]
    fn equal_lengths_snaps_to_grid_and_keeps_junctions_labels_and_undo() {
        let mut canvas = Canvas::default();
        canvas.grid_enabled = true;
        canvas.elements = vec![
            Element::Resistor(egui::pos2(13.0, 23.0), egui::pos2(103.0, 23.0)),
            Element::Resistor(egui::pos2(103.0, 23.0), egui::pos2(103.0, 213.0)),
            Element::Resistor(egui::pos2(103.0, 23.0), egui::pos2(303.0, 313.0)),
        ];
        canvas.labels = vec![String::new(); 3];
        canvas.selected_endpoints = vec![
            DraggedEndpoint {
                element_index: 0,
                is_start: true,
            },
            DraggedEndpoint {
                element_index: 0,
                is_start: false,
            },
            DraggedEndpoint {
                element_index: 1,
                is_start: false,
            },
        ];
        canvas.node_labels.push(crate::label::NodeLabel {
            position: egui::pos2(103.0, 23.0),
            text: "A".into(),
        });
        canvas.reset_history();
        let before = canvas.drawing();
        canvas.equalize_selected_lengths();
        for endpoint in &canvas.selected_endpoints {
            let (start, end) = canvas.elements[endpoint.element_index].endpoints();
            let position = if endpoint.is_start { start } else { end };
            assert_eq!(position.x % GRID_SPACING, 0.0);
            assert_eq!(position.y % GRID_SPACING, 0.0);
        }
        let junction = canvas.elements[0].endpoints().1;
        assert_eq!(canvas.elements[1].endpoints().0, junction);
        assert_eq!(canvas.elements[2].endpoints().0, junction);
        assert_eq!(canvas.node_labels[0].position, junction);
        assert_eq!(canvas.elements[2].endpoints().1, egui::pos2(303.0, 313.0));
        let after = canvas.drawing();
        canvas.undo();
        assert_eq!(canvas.drawing(), before);
        canvas.redo();
        assert_eq!(canvas.drawing(), after);
    }

    #[test]
    fn alignment_moves_only_selected_coordinates_and_attached_labels() {
        for horizontal in [true, false] {
            let mut canvas = Canvas::default();
            canvas.elements = vec![
                Element::Resistor(egui::pos2(10.0, 20.0), egui::pos2(100.0, 100.0)),
                Element::PrescribedFlow(egui::pos2(200.0, 200.0), egui::pos2(50.0, 80.0)),
            ];
            canvas.selected_endpoints = vec![
                DraggedEndpoint {
                    element_index: 0,
                    is_start: true,
                },
                DraggedEndpoint {
                    element_index: 1,
                    is_start: false,
                },
            ];
            canvas.terminal_labels.push(crate::label::NodeLabel {
                position: egui::pos2(10.0, 20.0),
                text: "terminal".into(),
            });
            canvas.node_labels.push(crate::label::NodeLabel {
                position: egui::pos2(50.0, 80.0),
                text: "node".into(),
            });
            let selection = canvas.selected_endpoints.clone();
            canvas.align_selected_endpoints(horizontal);
            let expected = if horizontal {
                [egui::pos2(10.0, 50.0), egui::pos2(50.0, 50.0)]
            } else {
                [egui::pos2(30.0, 20.0), egui::pos2(30.0, 80.0)]
            };
            assert_eq!(
                canvas.elements[0].endpoints(),
                (expected[0], egui::pos2(100.0, 100.0))
            );
            assert_eq!(
                canvas.elements[1].endpoints(),
                (egui::pos2(200.0, 200.0), expected[1])
            );
            assert_eq!(canvas.terminal_labels[0].position, expected[0]);
            assert_eq!(canvas.node_labels[0].position, expected[1]);
            assert_eq!(canvas.selected_endpoints, selection);
        }
    }

    #[test]
    fn alignment_snaps_both_coordinates_to_grid_and_supports_undo() {
        for horizontal in [true, false] {
            let mut canvas = Canvas::default();
            canvas.grid_enabled = true;
            canvas.elements = vec![Element::Resistor(
                egui::pos2(13.0, 23.0),
                egui::pos2(87.0, 92.0),
            )];
            canvas.labels.push(String::new());
            canvas.selected_endpoints = vec![
                DraggedEndpoint {
                    element_index: 0,
                    is_start: true,
                },
                DraggedEndpoint {
                    element_index: 0,
                    is_start: false,
                },
            ];
            canvas.terminal_labels.push(crate::label::NodeLabel {
                position: egui::pos2(13.0, 23.0),
                text: "terminal".into(),
            });
            canvas.reset_history();
            let before = canvas.drawing();
            canvas.align_selected_endpoints(horizontal);
            let expected = if horizontal {
                (egui::pos2(20.0, 60.0), egui::pos2(80.0, 60.0))
            } else {
                (egui::pos2(60.0, 20.0), egui::pos2(60.0, 100.0))
            };
            assert_eq!(canvas.elements[0].endpoints(), expected);
            assert_eq!(canvas.terminal_labels[0].position, expected.0);
            let aligned = canvas.drawing();
            canvas.undo();
            assert_eq!(canvas.drawing(), before);
            canvas.redo();
            assert_eq!(canvas.drawing(), aligned);
        }
    }

    #[test]
    fn solve_runs_in_background_on_a_snapshot() {
        let mut canvas = Canvas::default();
        canvas.elements.push(Element::Resistor(
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
        ));
        canvas.labels.push("G".into());
        canvas.elements.push(Element::PrescribedFlow(
            egui::pos2(100.0, 50.0),
            egui::pos2(100.0, 0.0),
        ));
        canvas.labels.push("Q".into());
        canvas.terminal_labels.push(crate::label::NodeLabel {
            position: egui::pos2(0.0, 0.0),
            text: "T_0".into(),
        });
        canvas.selected_endpoints.push(DraggedEndpoint {
            element_index: 0,
            is_start: false,
        });
        canvas.start_solve(&egui::Context::default());
        // Edits after clicking Solve must not change the pending result.
        canvas.labels[0] = "K".into();
        let Some(SolveState::Running(receiver)) = &canvas.solve else {
            panic!("solve did not start in the background");
        };
        let result = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert_eq!(result.unwrap(), "p_selected = (G*T_0 + Q) / (G)");
    }

    #[test]
    fn alignment_requires_at_least_two_endpoints() {
        let mut canvas = Canvas::default();
        canvas.elements.push(Element::Resistor(
            egui::pos2(10.0, 20.0),
            egui::pos2(50.0, 80.0),
        ));
        let original = canvas.elements.clone();
        for horizontal in [true, false] {
            canvas.align_selected_endpoints(horizontal);
            assert_eq!(canvas.elements, original);
        }
        canvas.selected_endpoints.push(DraggedEndpoint {
            element_index: 0,
            is_start: true,
        });
        for horizontal in [true, false] {
            canvas.align_selected_endpoints(horizontal);
            assert_eq!(canvas.elements, original);
        }
    }
}
