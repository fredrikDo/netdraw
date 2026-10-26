//! Drawing geometry, endpoint interaction, and visual node discovery.
//!
//! `Resistor` names the historical zigzag shape; the solver interprets its label
//! as conductance. Visual junction markers do not define mathematical connectivity.

use eframe::egui;

// Every completed item on the canvas is stored with its two editable endpoints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Element {
    /// Zigzag geometry whose solver label is conductance, despite this legacy name.
    Resistor(egui::Pos2, egui::Pos2),
    /// Independent positive injection at the end endpoint; the start is decorative.
    PrescribedFlow(egui::Pos2, egui::Pos2),
}

impl Element {
    pub fn endpoints(&self) -> (egui::Pos2, egui::Pos2) {
        match self {
            Self::Resistor(start, end) | Self::PrescribedFlow(start, end) => (*start, *end),
        }
    }

    pub fn move_endpoint(&mut self, is_start: bool, position: egui::Pos2) {
        // Two-terminal elements share the same editable geometry.
        match self {
            Self::Resistor(start, end) | Self::PrescribedFlow(start, end) => {
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
            Self::Resistor(start, end) => Self::Resistor(start + offset, end + offset),
            Self::PrescribedFlow(start, end) => Self::PrescribedFlow(start + offset, end + offset),
        }
    }
}

// Selects which kind of element a drag on empty canvas space creates.
#[derive(Clone, Copy, PartialEq)]
pub enum DrawMode {
    Resistor,
    PrescribedFlow,
    Select,
    Label,
    Erase,
}

/// Element index plus endpoint side; valid only while the element ordering is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraggedEndpoint {
    pub element_index: usize,
    pub is_start: bool,
}

// Handles are drawn at this radius; their larger hit area makes them easier to grab.
pub const ENDPOINT_RADIUS: f32 = 8.0;
const RESISTOR_BODY_LENGTH: f32 = 60.0;
const RESISTOR_HALF_WIDTH: f32 = 10.0;

// The boundary tick spans exactly the same transverse width as the zigzag,
// including when a short resistor is scaled down.
pub fn resistor_half_width(length: f32) -> f32 {
    RESISTOR_HALF_WIDTH * (length / RESISTOR_BODY_LENGTH).min(1.0)
}
pub const RESISTOR_ZIG_SEGMENTS: usize = 7;
pub const DIAGRAM_STROKE_WIDTH: f32 = 1.0;
pub const JUNCTION_NODE_RADIUS: f32 = 2.0;

/// Three zigzag cycles with half-length first/last legs, giving every leg
/// the same slope. Shared by the canvas, SVG export, and export bounds.
pub fn resistor_body(start: egui::Pos2, end: egui::Pos2) -> Option<Vec<egui::Pos2>> {
    let offset = end - start;
    let length = offset.length();
    if length < 5.0 {
        return None;
    }
    let direction = offset / length;
    let perpendicular = egui::vec2(-direction.y, direction.x);
    let body_length = length.min(RESISTOR_BODY_LENGTH);
    let amplitude = resistor_half_width(length);
    let body_start = start + direction * ((length - body_length) / 2.0);
    let mut points = Vec::with_capacity(RESISTOR_ZIG_SEGMENTS + 1);
    points.push(body_start);
    for index in 0..RESISTOR_ZIG_SEGMENTS - 1 {
        let fraction = (index as f32 + 0.5) / (RESISTOR_ZIG_SEGMENTS - 1) as f32;
        let height = if index % 2 == 0 {
            -amplitude
        } else {
            amplitude
        };
        points.push(body_start + direction * (body_length * fraction) + perpendicular * height);
    }
    points.push(body_start + direction * body_length);
    Some(points)
}
// Shared range for endpoint snapping, hover indicators, and selection.
pub const ENDPOINT_INTERACTION_DISTANCE: f32 = 16.0;
const ERASE_DISTANCE: f32 = 10.0;

// Check the most recently drawn elements first, matching their visual stacking order.
pub fn endpoint_at(elements: &[Element], position: egui::Pos2) -> Option<DraggedEndpoint> {
    for (element_index, element) in elements.iter().enumerate().rev() {
        let (start, end) = element.endpoints();

        if position.distance(start) <= ENDPOINT_INTERACTION_DISTANCE {
            return Some(DraggedEndpoint {
                element_index,
                is_start: true,
            });
        }

        if position.distance(end) <= ENDPOINT_INTERACTION_DISTANCE {
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

/// Collect editable handles inside a selection rectangle in canvas coordinates.
pub fn endpoints_in_rect(elements: &[Element], rect: egui::Rect) -> Vec<DraggedEndpoint> {
    elements
        .iter()
        .enumerate()
        .flat_map(|(element_index, element)| {
            let (start, end) = element.endpoints();
            [(true, start), (false, end)]
                .into_iter()
                .filter_map(move |(is_start, endpoint)| {
                    rect.contains(endpoint).then_some(DraggedEndpoint {
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
                        Element::Resistor(other_start, other_end) => {
                            Some([*other_start, *other_end])
                        }
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

/// Resistor endpoints shared with no other resistor endpoint or arrow tip. These are
/// the only places terminal ticks and pressure-boundary labels appear, and the solver
/// requires a label at each one. Arrow tails are decorative and never connect.
pub fn unconnected_resistor_terminal_positions(elements: &[Element]) -> Vec<egui::Pos2> {
    let connections: Vec<_> = elements
        .iter()
        .flat_map(|element| match *element {
            Element::Resistor(start, end) => vec![start, end],
            Element::PrescribedFlow(_, tip) => vec![tip],
        })
        .collect();
    elements
        .iter()
        .filter_map(|element| match *element {
            Element::Resistor(start, end) => Some([start, end]),
            Element::PrescribedFlow(..) => None,
        })
        .flatten()
        .filter(|position| {
            connections
                .iter()
                .filter(|other| *other == position)
                .count()
                == 1
        })
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
            if distance <= ENDPOINT_INTERACTION_DISTANCE
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
    elements
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, element)| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resistor_has_uniform_slopes_and_scales_when_short() {
        for (end, scale) in [(egui::pos2(100.0, 0.0), 1.0), (egui::pos2(30.0, 0.0), 0.5)] {
            let points = resistor_body(egui::Pos2::ZERO, end).unwrap();
            assert_eq!(points.len(), 8);
            assert_eq!(points[0].y, 0.0);
            assert_eq!(points[7].y, 0.0);
            for point in &points[1..7] {
                assert_eq!(point.y.abs(), 10.0 * scale);
            }
            for pair in points.windows(2) {
                let delta = pair[1] - pair[0];
                assert!((delta.y.abs() / delta.x - 2.0).abs() < 0.001);
            }
            let vertical = resistor_body(egui::Pos2::ZERO, egui::pos2(0.0, end.x)).unwrap();
            for (horizontal, vertical) in points.iter().zip(vertical) {
                assert_eq!(vertical, egui::pos2(-horizontal.y, horizontal.x));
            }
        }
        assert!(resistor_body(egui::Pos2::ZERO, egui::Pos2::ZERO).is_none());
    }

    #[test]
    fn box_selection_returns_every_enclosed_endpoint() {
        let elements = [
            Element::Resistor(egui::pos2(10.0, 10.0), egui::pos2(100.0, 100.0)),
            Element::Resistor(egui::pos2(20.0, 20.0), egui::pos2(120.0, 120.0)),
        ];

        let selected = endpoints_in_rect(
            &elements,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(30.0, 30.0)),
        );

        assert_eq!(
            selected,
            [
                DraggedEndpoint {
                    element_index: 0,
                    is_start: true,
                },
                DraggedEndpoint {
                    element_index: 1,
                    is_start: true,
                },
            ]
        );
    }
}
