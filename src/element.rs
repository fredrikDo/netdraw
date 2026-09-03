use eframe::egui;

// Every completed item on the canvas is stored with its two editable endpoints.
#[derive(Clone, Copy)]
pub enum Element {
    Line(egui::Pos2, egui::Pos2),
    Resistor(egui::Pos2, egui::Pos2),
    PrescribedFlow(egui::Pos2, egui::Pos2),
}

impl Element {
    pub fn endpoints(&self) -> (egui::Pos2, egui::Pos2) {
        match self {
            Self::Line(start, end)
            | Self::Resistor(start, end)
            | Self::PrescribedFlow(start, end) => (*start, *end),
        }
    }

    pub fn move_endpoint(&mut self, is_start: bool, position: egui::Pos2) {
        // Two-terminal elements share the same editable geometry.
        match self {
            Self::Line(start, end)
            | Self::Resistor(start, end)
            | Self::PrescribedFlow(start, end) => {
                if is_start {
                    *start = position;
                } else {
                    *end = position;
                }
            }
        }
    }

    // A prescribed-flow arrow only connects at its tip (the end endpoint).
    pub fn endpoint_can_snap(&self, is_start: bool) -> bool {
        !matches!(self, Self::PrescribedFlow(_, _) if is_start)
    }

    pub fn translated(self, offset: egui::Vec2) -> Self {
        match self {
            Self::Line(start, end) => Self::Line(start + offset, end + offset),
            Self::Resistor(start, end) => Self::Resistor(start + offset, end + offset),
            Self::PrescribedFlow(start, end) => Self::PrescribedFlow(start + offset, end + offset),
        }
    }
}

// Selects which kind of element a drag on empty canvas space creates.
#[derive(Clone, Copy, PartialEq)]
pub enum DrawMode {
    Line,
    Resistor,
    PrescribedFlow,
    Move,
    Erase,
}

#[derive(Clone, Copy)]
pub struct DraggedEndpoint {
    pub element_index: usize,
    pub is_start: bool,
}

// Handles are drawn at this radius; their larger hit area makes them easier to grab.
pub const ENDPOINT_RADIUS: f32 = 8.0;
// Endpoints within this distance are joined to make circuit connections easy to align.
const SNAP_DISTANCE: f32 = 12.0;
const ERASE_DISTANCE: f32 = 10.0;

// Check the most recently drawn elements first, matching their visual stacking order.
pub fn endpoint_at(elements: &[Element], position: egui::Pos2) -> Option<DraggedEndpoint> {
    for (element_index, element) in elements.iter().enumerate().rev() {
        let (start, end) = element.endpoints();

        if position.distance(start) <= ENDPOINT_RADIUS * 2.0 {
            return Some(DraggedEndpoint {
                element_index,
                is_start: true,
            });
        }

        if position.distance(end) <= ENDPOINT_RADIUS * 2.0 {
            return Some(DraggedEndpoint {
                element_index,
                is_start: false,
            });
        }
    }

    None
}

// Returns every endpoint at the selected endpoint's position so joined elements move together.
pub fn overlapping_endpoints(
    elements: &[Element],
    selected: DraggedEndpoint,
) -> Vec<DraggedEndpoint> {
    let (start, end) = elements[selected.element_index].endpoints();
    let selected_position = if selected.is_start { start } else { end };

    elements
        .iter()
        .enumerate()
        .flat_map(|(element_index, element)| {
            let (start, end) = element.endpoints();
            [(true, start), (false, end)]
                .into_iter()
                .filter_map(move |(is_start, endpoint)| {
                    (endpoint == selected_position).then_some(DraggedEndpoint {
                        element_index,
                        is_start,
                    })
                })
        })
        .collect()
}

// Positions represented by the junction dots on the canvas.
pub fn junction_positions(elements: &[Element]) -> Vec<egui::Pos2> {
    let mut positions = Vec::new();

    for element in elements {
        if let Element::Resistor(start, end) = element {
            for position in [*start, *end] {
                let count = elements
                    .iter()
                    .filter_map(|other| match other {
                        Element::Resistor(other_start, other_end) => Some([*other_start, *other_end]),
                        _ => None,
                    })
                    .flatten()
                    .filter(|other| *other == position)
                    .count();
                if count >= 2 && !positions.contains(&position) {
                    positions.push(position);
                }
            }
        }
    }

    for element in elements {
        if let Element::PrescribedFlow(_, end) = element
            && !positions.contains(end)
        {
            positions.push(*end);
        }
    }

    positions
}

pub fn unconnected_resistor_terminal_positions(elements: &[Element]) -> Vec<egui::Pos2> {
    let all_endpoints: Vec<_> = elements
        .iter()
        .flat_map(|element| {
            let (start, end) = element.endpoints();
            [start, end]
        })
        .collect();

    elements
        .iter()
        .filter_map(|element| match element {
            Element::Resistor(start, end) => Some([*start, *end]),
            _ => None,
        })
        .flatten()
        .filter(|position| all_endpoints.iter().filter(|other| **other == *position).count() == 1)
        .collect()
}

// Returns the closest existing endpoint inside the snap radius, or the original position.
// `ignored_endpoints` prevents dragged handles from snapping to their old positions.
pub fn snap_to_endpoint(
    elements: &[Element],
    position: egui::Pos2,
    ignored_endpoints: &[DraggedEndpoint],
) -> egui::Pos2 {
    let mut closest = None;

    for (element_index, element) in elements.iter().enumerate() {
        let (start, end) = element.endpoints();

        for (is_start, endpoint) in [(true, start), (false, end)] {
            if !element.endpoint_can_snap(is_start) {
                continue;
            }

            if ignored_endpoints.iter().any(|ignored| {
                ignored.element_index == element_index && ignored.is_start == is_start
            }) {
                continue;
            }

            let distance = position.distance(endpoint);
            if distance <= SNAP_DISTANCE
                && closest.is_none_or(|(_, closest_distance)| distance < closest_distance)
            {
                closest = Some((endpoint, distance));
            }
        }
    }

    closest.map_or(position, |(endpoint, _)| endpoint)
}

// Finds the topmost element close enough to be erased.
pub fn element_at(elements: &[Element], position: egui::Pos2) -> Option<usize> {
    elements.iter().enumerate().rev().find_map(|(index, element)| {
        let (start, end) = element.endpoints();
        (distance_to_segment(position, start, end) <= ERASE_DISTANCE).then_some(index)
    })
}

// Returns the distance from a point to the closest point on a line segment.
fn distance_to_segment(point: egui::Pos2, start: egui::Pos2, end: egui::Pos2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_sq();

    if length_squared == 0.0 {
        return point.distance(start);
    }

    let position_on_segment = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * position_on_segment)
}
