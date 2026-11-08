//! Drawing geometry, endpoint interaction, and visual node discovery.
//!
//! `Resistor` names the historical zigzag shape; the solver interprets its label
//! as conductance. Visual junction markers do not define mathematical connectivity.

use eframe::egui;

/// One completed drawing item, stored by its two editable endpoints in canvas coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Element {
    /// Zigzag geometry whose solver label is conductance, despite this legacy name.
    Resistor(egui::Pos2, egui::Pos2),
    /// Independent positive injection at the end endpoint; the start is decorative.
    PrescribedFlow(egui::Pos2, egui::Pos2),
}

impl Element {
    /// Return `(start, end)` regardless of element kind.
    pub fn endpoints(&self) -> (egui::Pos2, egui::Pos2) {
        match self {
            Self::Resistor(start, end) | Self::PrescribedFlow(start, end) => (*start, *end),
        }
    }

    /// Return one endpoint, selected the same way as in [`DraggedEndpoint`].
    pub fn endpoint(&self, is_start: bool) -> egui::Pos2 {
        let (start, end) = self.endpoints();
        if is_start { start } else { end }
    }

    /// Move one endpoint; both element kinds share the same editable geometry.
    pub fn move_endpoint(&mut self, is_start: bool, position: egui::Pos2) {
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

    /// A prescribed-flow arrow only connects at its tip (the end endpoint).
    pub fn endpoint_can_snap(&self, is_start: bool) -> bool {
        !matches!(self, Self::PrescribedFlow(_, _) if is_start)
    }

    /// Shift both endpoints, e.g. from canvas to screen coordinates for painting.
    pub fn translated(self, offset: egui::Vec2) -> Self {
        match self {
            Self::Resistor(start, end) => Self::Resistor(start + offset, end + offset),
            Self::PrescribedFlow(start, end) => Self::PrescribedFlow(start + offset, end + offset),
        }
    }

    /// Positions that take part in connectivity: both resistor ends, but only an arrow's tip.
    fn connection_points(&self) -> impl Iterator<Item = egui::Pos2> {
        let (start, end) = self.endpoints();
        let start = matches!(self, Self::Resistor(..)).then_some(start);
        start.into_iter().chain([end])
    }
}

/// Selects which kind of element a drag on empty canvas space creates, or which
/// non-drawing interaction (select, label, erase) canvas clicks perform.
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

/// Radius at which endpoint handles are drawn; their hit area is larger (see below).
pub const ENDPOINT_RADIUS: f32 = 8.0;
/// Shared range for endpoint snapping, hover indicators, and selection.
pub const ENDPOINT_INTERACTION_DISTANCE: f32 = 16.0;
/// Pointer distance from an element's centre line that still hits it (erase, label).
pub const ELEMENT_HIT_DISTANCE: f32 = 10.0;
/// Elements shorter than this have no drawable symbol, so the canvas does not create them.
pub const MIN_ELEMENT_LENGTH: f32 = 5.0;
pub const DIAGRAM_STROKE_WIDTH: f32 = 1.0;
pub const JUNCTION_NODE_RADIUS: f32 = 2.0;
pub const RESISTOR_ZIG_SEGMENTS: usize = 7;
const RESISTOR_BODY_LENGTH: f32 = 60.0;
const RESISTOR_HALF_WIDTH: f32 = 10.0;
const PRESCRIBED_FLOW_RADIUS: f32 = 16.0;

/// Transverse half-width of the zigzag. Short resistors scale down proportionally,
/// and the boundary tick uses the same width so both always line up.
pub fn resistor_half_width(length: f32) -> f32 {
    RESISTOR_HALF_WIDTH * (length / RESISTOR_BODY_LENGTH).min(1.0)
}

/// Three zigzag cycles with half-length first/last legs, giving every leg
/// the same slope. Shared by the canvas, SVG export, and export bounds.
/// Returns `None` for elements too short to draw.
pub fn resistor_body(start: egui::Pos2, end: egui::Pos2) -> Option<Vec<egui::Pos2>> {
    let offset = end - start;
    let length = offset.length();
    if length < MIN_ELEMENT_LENGTH {
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

/// Geometry of the IEEE-style independent source symbol: lead wires, a centred
/// circle, and an arrow pointing from `start` toward the connected tip at `end`.
pub struct PrescribedFlowSymbol {
    pub leads: [[egui::Pos2; 2]; 2],
    pub center: egui::Pos2,
    pub radius: f32,
    /// Arrow shaft, ending at the arrowhead base so the line does not poke through the tip.
    pub shaft: [egui::Pos2; 2],
    /// Filled triangle: tip first, then the two base corners.
    pub head: [egui::Pos2; 3],
}

/// Shared by the canvas, SVG export, and export bounds. `None` for a zero-length arrow.
pub fn prescribed_flow_symbol(start: egui::Pos2, end: egui::Pos2) -> Option<PrescribedFlowSymbol> {
    let offset = end - start;
    let length = offset.length();
    if length == 0.0 {
        return None;
    }
    let direction = offset / length;
    let perpendicular = egui::vec2(-direction.y, direction.x);
    let center = start + offset * 0.5;
    let radius = PRESCRIBED_FLOW_RADIUS.min(length / 2.0);
    // Arrow proportions are fractions of the circle radius so short symbols scale down.
    let arrow_tip = center + direction * (radius * 0.55);
    let head_base = arrow_tip - direction * (radius * 0.4);
    let head_half_width = radius * 0.22;
    Some(PrescribedFlowSymbol {
        leads: [
            [start, center - direction * radius],
            [center + direction * radius, end],
        ],
        center,
        radius,
        shaft: [center - direction * (radius * 0.55), head_base],
        head: [
            arrow_tip,
            head_base + perpendicular * head_half_width,
            head_base - perpendicular * head_half_width,
        ],
    })
}

/// Check the most recently drawn elements first, matching their visual stacking order.
pub fn endpoint_at(elements: &[Element], position: egui::Pos2) -> Option<DraggedEndpoint> {
    elements
        .iter()
        .enumerate()
        .rev()
        .find_map(|(element_index, element)| {
            [true, false].into_iter().find_map(|is_start| {
                (position.distance(element.endpoint(is_start)) <= ENDPOINT_INTERACTION_DISTANCE)
                    .then_some(DraggedEndpoint {
                        element_index,
                        is_start,
                    })
            })
        })
}

/// Every endpoint for which `keep` returns true, in element order, start before end.
fn endpoints_where(
    elements: &[Element],
    keep: impl Fn(egui::Pos2) -> bool,
) -> Vec<DraggedEndpoint> {
    elements
        .iter()
        .enumerate()
        .flat_map(|(element_index, element)| {
            [true, false].map(|is_start| (element_index, is_start, element.endpoint(is_start)))
        })
        .filter(|&(_, _, position)| keep(position))
        .map(|(element_index, is_start, _)| DraggedEndpoint {
            element_index,
            is_start,
        })
        .collect()
}

/// Return every endpoint at the selected endpoint's position so joined elements move together.
pub fn overlapping_endpoints(
    elements: &[Element],
    selected: DraggedEndpoint,
) -> Vec<DraggedEndpoint> {
    let selected_position = elements[selected.element_index].endpoint(selected.is_start);
    endpoints_where(elements, |position| position == selected_position)
}

/// Collect editable handles inside a selection rectangle in canvas coordinates.
pub fn endpoints_in_rect(elements: &[Element], rect: egui::Rect) -> Vec<DraggedEndpoint> {
    endpoints_where(elements, |position| rect.contains(position))
}

/// Distinct positions and how many items of `positions` coincide with each, in first-seen
/// order. Topology uses exact coordinate identity, so a linear scan is the simplest
/// correct grouping (f32 positions are not `Hash`), and drawings are small.
fn position_counts(positions: impl Iterator<Item = egui::Pos2>) -> Vec<(egui::Pos2, usize)> {
    let mut counts: Vec<(egui::Pos2, usize)> = Vec::new();
    for position in positions {
        match counts
            .iter_mut()
            .find(|(existing, _)| *existing == position)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((position, 1)),
        }
    }
    counts
}

/// Positions drawn with a junction dot: where two or more resistor endpoints meet, and
/// every arrow tip. These are also the places an internal node label can attach.
/// The dots are a visual aid only; the solver builds its own graph.
pub fn junction_positions(elements: &[Element]) -> Vec<egui::Pos2> {
    let resistor_ends = elements.iter().flat_map(|element| match *element {
        Element::Resistor(start, end) => vec![start, end],
        Element::PrescribedFlow(..) => vec![],
    });
    let mut positions: Vec<_> = position_counts(resistor_ends)
        .into_iter()
        .filter(|&(_, count)| count >= 2)
        .map(|(position, _)| position)
        .collect();
    for element in elements {
        if let Element::PrescribedFlow(_, tip) = *element
            && !positions.contains(&tip)
        {
            positions.push(tip);
        }
    }
    positions
}

/// Resistor endpoints shared with no other resistor endpoint or arrow tip. These are
/// the only places terminal ticks and pressure-boundary labels appear, and the solver
/// requires a label at each one. Arrow tails are decorative and never connect.
pub fn unconnected_resistor_terminal_positions(elements: &[Element]) -> Vec<egui::Pos2> {
    let counts = position_counts(elements.iter().flat_map(Element::connection_points));
    elements
        .iter()
        .filter_map(|element| match *element {
            Element::Resistor(start, end) => Some([start, end]),
            Element::PrescribedFlow(..) => None,
        })
        .flatten()
        .filter(|position| counts.contains(&(*position, 1)))
        .collect()
}

/// Line segments marking each free resistor terminal, perpendicular to its resistor and
/// as wide as the zigzag. Shared by the canvas, SVG export, and export bounds.
pub fn terminal_ticks(elements: &[Element]) -> Vec<[egui::Pos2; 2]> {
    let terminals = unconnected_resistor_terminal_positions(elements);
    let mut ticks = Vec::with_capacity(terminals.len());
    for element in elements {
        let Element::Resistor(start, end) = *element else {
            continue;
        };
        let direction = end - start;
        let length = direction.length();
        if length == 0.0 {
            continue;
        }
        let half_tick =
            egui::vec2(-direction.y, direction.x) / length * resistor_half_width(length);
        for position in [start, end] {
            if terminals.contains(&position) {
                ticks.push([position - half_tick, position + half_tick]);
            }
        }
    }
    ticks
}

/// Return the closest existing endpoint inside the snap radius, or the original position.
/// `ignored_endpoints` prevents dragged handles from snapping to their old positions.
pub fn snap_to_endpoint(
    elements: &[Element],
    position: egui::Pos2,
    ignored_endpoints: &[DraggedEndpoint],
) -> egui::Pos2 {
    let mut closest = None;

    for (element_index, element) in elements.iter().enumerate() {
        for is_start in [true, false] {
            if !element.endpoint_can_snap(is_start)
                || ignored_endpoints.contains(&DraggedEndpoint {
                    element_index,
                    is_start,
                })
            {
                continue;
            }

            let endpoint = element.endpoint(is_start);
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

/// Find the topmost element whose centre line passes within [`ELEMENT_HIT_DISTANCE`].
pub fn element_at(elements: &[Element], position: egui::Pos2) -> Option<usize> {
    elements
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, element)| {
            let (start, end) = element.endpoints();
            (distance_to_segment(position, start, end) <= ELEMENT_HIT_DISTANCE).then_some(index)
        })
}

/// Distance from a point to the closest point on a line segment.
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
    fn junctions_and_terminals_follow_connectivity() {
        let shared = egui::pos2(100.0, 0.0);
        let elements = [
            Element::Resistor(egui::pos2(0.0, 0.0), shared),
            Element::Resistor(shared, egui::pos2(200.0, 0.0)),
            // Arrow tail at a free terminal does not connect it; the tip makes a junction.
            Element::PrescribedFlow(egui::pos2(0.0, 0.0), egui::pos2(300.0, 50.0)),
        ];

        assert_eq!(
            junction_positions(&elements),
            [shared, egui::pos2(300.0, 50.0)]
        );
        assert_eq!(
            unconnected_resistor_terminal_positions(&elements),
            [egui::pos2(0.0, 0.0), egui::pos2(200.0, 0.0)]
        );
        // Ticks are vertical here and as wide as the full-size zigzag.
        assert_eq!(
            terminal_ticks(&elements)[0],
            [egui::pos2(0.0, -10.0), egui::pos2(0.0, 10.0)]
        );
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
