use eframe::egui;

use crate::element::Element;

const FONT_HEIGHT: f32 = 18.0;
const CHARACTER_WIDTH: f32 = 8.0;
const ELEMENT_CLEARANCE: f32 = 8.0;

#[derive(Clone, Copy)]
pub struct Placement {
    pub rect: egui::Rect,
}

#[derive(Clone)]
pub struct NodeLabel {
    pub position: egui::Pos2,
    pub text: String,
}

pub fn placements(elements: &[Element], labels: &[String]) -> Vec<Option<Placement>> {
    let mut placed: Vec<Option<Placement>> = Vec::with_capacity(elements.len());

    for (index, element) in elements.iter().enumerate() {
        let label = labels.get(index).map(String::as_str).unwrap_or_default();
        if label.is_empty() {
            placed.push(None);
            continue;
        }

        let size = egui::vec2(label.chars().count() as f32 * CHARACTER_WIDTH + 4.0, FONT_HEIGHT);
        let (start, end) = element.endpoints();
        let offset = end - start;
        let direction = if offset.length_sq() > 0.0 {
            offset.normalized()
        } else {
            egui::Vec2::X
        };
        let perpendicular = egui::vec2(-direction.y, direction.x);
        let center = start + offset * 0.5;

        let mut placement = None;
        // Try both sides, increasing the distance until the label clears every element and label.
        for distance in [32.0, 50.0, 68.0, 86.0, 104.0] {
            for side in [1.0, -1.0] {
                let rect = egui::Rect::from_center_size(center + perpendicular * distance * side, size);
                if elements.iter().all(|other| clears_element(rect, *other))
                    && placed.iter().flatten().all(|other| !rect.intersects(other.rect))
                {
                    placement = Some(Placement { rect });
                    break;
                }
            }
            if placement.is_some() {
                break;
            }
        }

        // A densely packed diagram may leave no completely clear candidate. Keep the label near
        // its element, choosing the least surprising side rather than hiding it.
        placed.push(placement.or_else(|| {
            Some(Placement {
                rect: egui::Rect::from_center_size(center + perpendicular * 32.0, size),
            })
        }));
    }

    placed
}

pub fn node_placements(
    elements: &[Element],
    node_labels: &[NodeLabel],
    occupied: &[Option<Placement>],
) -> Vec<Option<Placement>> {
    let mut placed: Vec<Option<Placement>> = occupied.to_vec();
    let mut node_placements = Vec::with_capacity(node_labels.len());

    for node_label in node_labels {
        if node_label.text.is_empty() {
            node_placements.push(None);
            continue;
        }
        let size = egui::vec2(
            node_label.text.chars().count() as f32 * CHARACTER_WIDTH + 4.0,
            FONT_HEIGHT,
        );
        let mut placement = None;
        for distance in [28.0, 46.0, 64.0, 82.0, 100.0] {
            for offset in [
                egui::Vec2::Y,
                -egui::Vec2::Y,
                egui::Vec2::X,
                -egui::Vec2::X,
                egui::vec2(0.7, 0.7),
                egui::vec2(-0.7, -0.7),
            ] {
                let rect = egui::Rect::from_center_size(node_label.position + offset * distance, size);
                if elements.iter().all(|element| clears_element(rect, *element))
                    && placed.iter().flatten().all(|other| !rect.intersects(other.rect))
                {
                    placement = Some(Placement { rect });
                    break;
                }
            }
            if placement.is_some() {
                break;
            }
        }
        let placement = placement.or_else(|| {
            Some(Placement {
                rect: egui::Rect::from_center_size(node_label.position + egui::Vec2::Y * 28.0, size),
            })
        });
        placed.push(placement);
        node_placements.push(placement);
    }

    node_placements
}

fn clears_element(rect: egui::Rect, element: Element) -> bool {
    let (start, end) = element.endpoints();
    !segment_intersects_rect(start, end, rect.expand(ELEMENT_CLEARANCE))
}

fn segment_intersects_rect(start: egui::Pos2, end: egui::Pos2, rect: egui::Rect) -> bool {
    rect.contains(start)
        || rect.contains(end)
        || [
            (rect.left_top(), rect.right_top()),
            (rect.right_top(), rect.right_bottom()),
            (rect.right_bottom(), rect.left_bottom()),
            (rect.left_bottom(), rect.left_top()),
        ]
        .into_iter()
        .any(|(edge_start, edge_end)| segments_intersect(start, end, edge_start, edge_end))
}

fn segments_intersect(
    first_start: egui::Pos2,
    first_end: egui::Pos2,
    second_start: egui::Pos2,
    second_end: egui::Pos2,
) -> bool {
    let first = first_end - first_start;
    let second = second_end - second_start;
    let denominator = cross(first, second);
    if denominator.abs() < f32::EPSILON {
        return false;
    }

    let between_starts = second_start - first_start;
    let first_progress = cross(between_starts, second) / denominator;
    let second_progress = cross(between_starts, first) / denominator;
    (0.0..=1.0).contains(&first_progress) && (0.0..=1.0).contains(&second_progress)
}

fn cross(first: egui::Vec2, second: egui::Vec2) -> f32 {
    first.x * second.y - first.y * second.x
}
