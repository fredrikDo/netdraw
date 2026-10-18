use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::{
    element::Element,
    label::{LabelFont, LabelStyle, NodeLabel},
};

const CURRENT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub elements: Vec<Element>,
    pub labels: Vec<String>,
    pub node_labels: Vec<NodeLabel>,
    pub terminal_labels: Vec<NodeLabel>,
    pub label_style: LabelStyle,
}

impl Drawing {
    pub fn empty() -> Self {
        Self {
            elements: Vec::new(),
            labels: Vec::new(),
            node_labels: Vec::new(),
            terminal_labels: Vec::new(),
            label_style: LabelStyle::default(),
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&SavedDrawing::from(self))
            .map_err(|error| format!("Could not encode drawing: {error}"))
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let saved: SavedDrawing =
            serde_json::from_str(json).map_err(|error| format!("Invalid NetDraw file: {error}"))?;
        saved.try_into()
    }
}

#[derive(Serialize, Deserialize)]
struct SavedDrawing {
    version: u32,
    elements: Vec<SavedElement>,
    labels: Vec<String>,
    node_labels: Vec<SavedLabel>,
    terminal_labels: Vec<SavedLabel>,
    label_style: SavedLabelStyle,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SavedElement {
    Line { start: [f32; 2], end: [f32; 2] },
    Resistor { start: [f32; 2], end: [f32; 2] },
    PrescribedFlow { start: [f32; 2], end: [f32; 2] },
}

#[derive(Serialize, Deserialize)]
struct SavedLabel {
    position: [f32; 2],
    text: String,
}

#[derive(Serialize, Deserialize)]
struct SavedLabelStyle {
    font: SavedFont,
    size: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SavedFont {
    #[serde(
        alias = "noto_sans",
        alias = "cantarell",
        alias = "comfortaa",
        alias = "deja_vu_sans_condensed"
    )]
    Inter,
    #[serde(rename = "source_serif_4", alias = "latin_modern")]
    SourceSerif4,
    #[serde(alias = "fira_mono")]
    IbmPlexMono,
    AtkinsonHyperlegible,
}

impl From<&Drawing> for SavedDrawing {
    fn from(drawing: &Drawing) -> Self {
        Self {
            version: CURRENT_VERSION,
            elements: drawing.elements.iter().copied().map(Into::into).collect(),
            labels: drawing.labels.clone(),
            node_labels: drawing.node_labels.iter().map(Into::into).collect(),
            terminal_labels: drawing.terminal_labels.iter().map(Into::into).collect(),
            label_style: drawing.label_style.into(),
        }
    }
}

impl TryFrom<SavedDrawing> for Drawing {
    type Error = String;

    fn try_from(saved: SavedDrawing) -> Result<Self, Self::Error> {
        if saved.version != CURRENT_VERSION {
            return Err(format!(
                "Unsupported NetDraw file version {} (this app supports version {CURRENT_VERSION}).",
                saved.version
            ));
        }
        if saved.labels.len() != saved.elements.len() {
            return Err(String::from(
                "Invalid NetDraw file: every element must have exactly one label entry.",
            ));
        }

        let drawing = Self {
            elements: saved.elements.into_iter().map(Into::into).collect(),
            labels: saved.labels,
            node_labels: saved.node_labels.into_iter().map(Into::into).collect(),
            terminal_labels: saved.terminal_labels.into_iter().map(Into::into).collect(),
            label_style: saved.label_style.into(),
        };
        drawing.validate()?;
        Ok(drawing)
    }
}

impl Drawing {
    fn validate(&self) -> Result<(), String> {
        let positions = self
            .elements
            .iter()
            .flat_map(|element| {
                let (start, end) = element.endpoints();
                [start, end]
            })
            .chain(self.node_labels.iter().map(|label| label.position))
            .chain(self.terminal_labels.iter().map(|label| label.position));
        if positions
            .into_iter()
            .any(|position| !position.x.is_finite() || !position.y.is_finite())
            || !self.label_style.size.is_finite()
            || !(8.0..=48.0).contains(&self.label_style.size)
        {
            return Err(String::from(
                "Invalid NetDraw file: coordinates or typography are out of range.",
            ));
        }
        Ok(())
    }
}

impl From<Element> for SavedElement {
    fn from(element: Element) -> Self {
        let point = |position: egui::Pos2| [position.x, position.y];
        match element {
            Element::Line(start, end) => Self::Line {
                start: point(start),
                end: point(end),
            },
            Element::Resistor(start, end) => Self::Resistor {
                start: point(start),
                end: point(end),
            },
            Element::PrescribedFlow(start, end) => Self::PrescribedFlow {
                start: point(start),
                end: point(end),
            },
        }
    }
}

impl From<SavedElement> for Element {
    fn from(element: SavedElement) -> Self {
        let point = |position: [f32; 2]| egui::pos2(position[0], position[1]);
        match element {
            SavedElement::Line { start, end } => Self::Line(point(start), point(end)),
            SavedElement::Resistor { start, end } => Self::Resistor(point(start), point(end)),
            SavedElement::PrescribedFlow { start, end } => {
                Self::PrescribedFlow(point(start), point(end))
            }
        }
    }
}

impl From<&NodeLabel> for SavedLabel {
    fn from(label: &NodeLabel) -> Self {
        Self {
            position: [label.position.x, label.position.y],
            text: label.text.clone(),
        }
    }
}

impl From<SavedLabel> for NodeLabel {
    fn from(label: SavedLabel) -> Self {
        Self {
            position: egui::pos2(label.position[0], label.position[1]),
            text: label.text,
        }
    }
}

impl From<LabelStyle> for SavedLabelStyle {
    fn from(style: LabelStyle) -> Self {
        Self {
            font: style.font.into(),
            size: style.size,
        }
    }
}

impl From<SavedLabelStyle> for LabelStyle {
    fn from(style: SavedLabelStyle) -> Self {
        Self {
            font: style.font.into(),
            size: style.size,
        }
    }
}

impl From<LabelFont> for SavedFont {
    fn from(font: LabelFont) -> Self {
        match font {
            LabelFont::Inter => Self::Inter,
            LabelFont::SourceSerif4 => Self::SourceSerif4,
            LabelFont::IbmPlexMono => Self::IbmPlexMono,
            LabelFont::AtkinsonHyperlegible => Self::AtkinsonHyperlegible,
        }
    }
}

impl From<SavedFont> for LabelFont {
    fn from(font: SavedFont) -> Self {
        match font {
            SavedFont::Inter => Self::Inter,
            SavedFont::SourceSerif4 => Self::SourceSerif4,
            SavedFont::IbmPlexMono => Self::IbmPlexMono,
            SavedFont::AtkinsonHyperlegible => Self::AtkinsonHyperlegible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_round_trips_through_json() {
        let drawing = Drawing {
            elements: vec![
                Element::Line(egui::pos2(1.0, 2.0), egui::pos2(3.0, 4.0)),
                Element::Resistor(egui::pos2(3.0, 4.0), egui::pos2(8.0, 9.0)),
                Element::PrescribedFlow(egui::pos2(7.0, 6.0), egui::pos2(3.0, 4.0)),
            ],
            labels: vec!["wire".into(), "R_1".into(), "q".into()],
            node_labels: vec![NodeLabel {
                position: egui::pos2(3.0, 4.0),
                text: "A".into(),
            }],
            terminal_labels: vec![NodeLabel {
                position: egui::pos2(8.0, 9.0),
                text: "B".into(),
            }],
            label_style: LabelStyle {
                font: LabelFont::IbmPlexMono,
                size: 21.0,
            },
        };

        let json = drawing.to_json().unwrap();
        assert_eq!(Drawing::from_json(&json).unwrap(), drawing);
    }

    #[test]
    fn all_current_fonts_round_trip_and_legacy_fonts_migrate() {
        for font in LabelFont::ALL {
            let mut drawing = Drawing::empty();
            drawing.label_style.font = font;
            assert_eq!(
                Drawing::from_json(&drawing.to_json().unwrap()).unwrap(),
                drawing
            );
        }
        let json = Drawing::empty().to_json().unwrap();
        for (old, expected) in [
            ("noto_sans", LabelFont::Inter),
            ("cantarell", LabelFont::Inter),
            ("comfortaa", LabelFont::Inter),
            ("deja_vu_sans_condensed", LabelFont::Inter),
            ("fira_mono", LabelFont::IbmPlexMono),
            ("latin_modern", LabelFont::SourceSerif4),
        ] {
            let old_json = json.replace("\"inter\"", &format!("\"{old}\""));
            let drawing = Drawing::from_json(&old_json).unwrap();
            assert_eq!(drawing.label_style.font, expected);
            assert_eq!(drawing.label_style.size, 22.0);
            assert!(!drawing.to_json().unwrap().contains(old));
        }
    }

    #[test]
    fn rejects_mismatched_element_labels() {
        let json = r#"{"version":1,"elements":[{"type":"line","start":[0,0],"end":[1,1]}],"labels":[],"node_labels":[],"terminal_labels":[],"label_style":{"font":"inter","size":18}}"#;
        assert!(Drawing::from_json(json).is_err());
    }
}
