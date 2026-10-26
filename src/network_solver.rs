//! Converts drawing topology into equations. All algebra lives in `symbolic`.
use crate::{
    document::Drawing,
    element::{DraggedEndpoint, Element, unconnected_resistor_terminal_positions},
    symbolic::{self, Expression},
};
use eframe::egui::Pos2;
use std::collections::BTreeMap;
// Count mathematical unknowns, not drawn endpoints or boundaries.
const MAX_UNKNOWNS: usize = 6;

fn endpoint_position(drawing: &Drawing, endpoint: &DraggedEndpoint) -> Option<Pos2> {
    drawing.elements.get(endpoint.element_index).map(|e| {
        let (a, b) = e.endpoints();
        if endpoint.is_start { a } else { b }
    })
}
/// Trim a literal symbol name; do not interpret label typography. Numeric labels are
/// rejected because the solver would otherwise confuse a symbol named `0` with zero.
fn required_symbol(label: Option<&String>, description: &str) -> Result<Expression, String> {
    let name = label
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Add a label to {description}."))?;
    if name.parse::<f64>().is_ok() && name.contains(|c: char| c.is_ascii_digit()) {
        return Err(format!(
            "The label \"{name}\" is a number. Use a symbol name such as T_0 for {description}."
        ));
    }
    Ok(Expression::symbol(name))
}

type Edge = (usize, usize, usize);
type Equations = (Vec<Vec<Expression>>, Vec<Expression>);

/// Each distinct connectable position is one node; its ID is its index here.
struct Topology {
    positions: Vec<Pos2>,
}
impl Topology {
    fn node_at(&self, position: Pos2) -> Option<usize> {
        self.positions.iter().position(|p| *p == position)
    }
}
/// Arrow tails and unsplit crossings create no nodes.
fn drawing_topology(drawing: &Drawing) -> Topology {
    let mut positions = Vec::new();
    for e in &drawing.elements {
        let endpoints = match *e {
            Element::Resistor(a, b) => vec![a, b],
            Element::PrescribedFlow(_, tip) => vec![tip],
        };
        for p in endpoints {
            if !positions.contains(&p) {
                positions.push(p);
            }
        }
    }
    Topology { positions }
}
/// Restrict solving and validation to conductances reachable from the selection.
fn connected_component(target: usize, edges: &[Edge]) -> Vec<usize> {
    let mut component = vec![target];
    loop {
        let before = component.len();
        for (_, a, b) in edges {
            if component.contains(a) || component.contains(b) {
                for n in [a, b] {
                    if !component.contains(n) {
                        component.push(*n);
                    }
                }
            }
        }
        if component.len() == before {
            break;
        }
    }
    component
}
/// Read known terminal values and reject conflicting or missing boundary labels.
/// Free terminals are exactly the positions the canvas marks with a terminal tick;
/// empty labels (e.g. a cancelled label edit) count as absent.
fn boundary_pressure(
    drawing: &Drawing,
    topology: &Topology,
    node: usize,
) -> Result<Option<Expression>, String> {
    let labels: Vec<_> = drawing
        .terminal_labels
        .iter()
        .filter(|l| topology.node_at(l.position) == Some(node) && !l.text.trim().is_empty())
        .collect();
    if labels.len() > 1
        && labels
            .iter()
            .any(|l| l.text.trim() != labels[0].text.trim())
    {
        return Err("Connected pressure boundaries have conflicting labels.".into());
    }
    if let Some(label) = labels.first() {
        return required_symbol(Some(&label.text), "the pressure boundary").map(Some);
    }
    let position = topology.positions[node];
    if unconnected_resistor_terminal_positions(&drawing.elements).contains(&position) {
        let conductance = drawing
            .elements
            .iter()
            .zip(&drawing.labels)
            .find_map(|(e, label)| match *e {
                Element::Resistor(a, b) if a == position || b == position => {
                    Some(label.trim()).filter(|l| !l.is_empty())
                }
                _ => None,
            });
        return Err(match conductance {
            Some(name) => {
                format!("Label the free terminal of conductance {name} with its known pressure.")
            }
            None => {
                "Label every free terminal (marked with a tick) with its known pressure.".into()
            }
        });
    }
    Ok(None)
}

/// Assemble the physical system using literal prescribed-flux symbols.
fn build_equations(
    drawing: &Drawing,
    topology: &Topology,
    edges: &[Edge],
    known: &[(usize, Option<Expression>)],
    unknown: &[usize],
) -> Result<Equations, String> {
    build_equations_with_sources(drawing, topology, edges, known, unknown, |i| {
        required_symbol(drawing.labels.get(i), "each prescribed flow")
    })
}

/// Assemble `A*x = b`; the callback substitutes physical fluxes or unit sources.
/// Known boundary terms enter `b`; conductances enter `A` independently of flux.
fn build_equations_with_sources(
    drawing: &Drawing,
    topology: &Topology,
    edges: &[Edge],
    known: &[(usize, Option<Expression>)],
    unknown: &[usize],
    source: impl Fn(usize) -> Result<Expression, String>,
) -> Result<Equations, String> {
    let mut matrix = vec![vec![Expression::integer(0); unknown.len()]; unknown.len()];
    let mut rhs = vec![Expression::integer(0); unknown.len()];
    for (row, &node) in unknown.iter().enumerate() {
        for (i, a, b) in edges {
            if a == b || (*a != node && *b != node) {
                continue;
            }
            let neighbor = if *a == node { *b } else { *a };
            let conductance = required_symbol(drawing.labels.get(*i), "each conductance")?;
            // G*(p_i - p_j): the self coefficient is positive, the neighbor negative.
            matrix[row][row] = matrix[row][row].add(&conductance)?;
            if let Some(col) = unknown.iter().position(|n| *n == neighbor) {
                matrix[row][col] = matrix[row][col].sub(&conductance)?;
            } else if let Some((_, Some(p))) = known.iter().find(|(n, _)| *n == neighbor) {
                rhs[row] = rhs[row].add(&conductance.mul(p)?)?;
            }
        }
        for (i, e) in drawing.elements.iter().enumerate() {
            if let Element::PrescribedFlow(_, p) = e {
                if topology.node_at(*p) == Some(node) {
                    // A prescribed flux never multiplies a node value or enters the matrix.
                    rhs[row] = rhs[row].add(&source(i)?)?;
                }
            }
        }
    }
    Ok((matrix, rhs))
}

/// Pick a repeated boundary value as the reference, then solve independent unit
/// responses. Boundary differences and prescribed fluxes remain outside the algebra.
fn reference_solution(
    drawing: &Drawing,
    topology: &Topology,
    edges: &[Edge],
    known: &[(usize, Option<Expression>)],
    unknown: &[usize],
    target: usize,
) -> Result<Option<String>, String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for (_, pressure) in known {
        if let Some(pressure) = pressure {
            *counts.entry(pressure.text()).or_default() += 1;
        }
    }
    let Some((reference, _)) = counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .max_by_key(|(_, count)| **count)
    else {
        return Ok(None);
    };
    // The matrix is the same for every unit response; only the right-hand side changes,
    // so collect them all and solve them in a single elimination.
    let mut matrix = Vec::new();
    let mut responses = Vec::new();
    let mut multipliers = Vec::new();
    for boundary in counts.keys().filter(|name| *name != reference) {
        let unit_boundaries = unit_boundary_values(known, Some(boundary));
        let (a, rhs) = build_equations_with_sources(
            drawing,
            topology,
            edges,
            &unit_boundaries,
            unknown,
            |_| Ok(Expression::integer(0)),
        )?;
        matrix = a;
        responses.push(rhs);
        multipliers.push(format!("({boundary} - {reference})"));
    }
    let flow_names: std::collections::BTreeSet<_> = drawing
        .elements
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Element::PrescribedFlow(_, position)
                if topology
                    .node_at(*position)
                    .is_some_and(|node| unknown.contains(&node)) =>
            {
                Some(drawing.labels[i].trim().to_string())
            }
            _ => None,
        })
        .collect();
    for flow in flow_names {
        let zero_boundaries = unit_boundary_values(known, None);
        let (a, rhs) = build_equations_with_sources(
            drawing,
            topology,
            edges,
            &zero_boundaries,
            unknown,
            |i| {
                Ok(Expression::integer(i32::from(
                    drawing.labels[i].trim() == flow,
                )))
            },
        )?;
        matrix = a;
        responses.push(rhs);
        multipliers.push(flow);
    }
    let mut terms = vec![reference.clone()];
    if !responses.is_empty() {
        let coefficients = symbolic::compact_solutions(matrix, responses, target)?;
        for (coefficient, multiplier) in coefficients.iter().zip(multipliers) {
            if coefficient != "0" {
                terms.push(format!("({coefficient})*{multiplier}"));
            }
        }
    }
    Ok(Some(terms.join(" + ")))
}

/// Preserve unknown nodes and replace known values by a selected unit excitation.
fn unit_boundary_values(
    known: &[(usize, Option<Expression>)],
    active: Option<&str>,
) -> Vec<(usize, Option<Expression>)> {
    known
        .iter()
        .map(|(node, pressure)| {
            (
                *node,
                pressure.as_ref().map(|value| {
                    Expression::integer(i32::from(active == Some(value.text().as_str())))
                }),
            )
        })
        .collect()
}

/// Validate and solve one selected mathematical node without mutating the drawing.
/// Returns a display expression or a diagnostic suitable for the result dialog.
pub fn solve_selected(drawing: &Drawing, selection: &[DraggedEndpoint]) -> Result<String, String> {
    let topology = drawing_topology(drawing);
    let node_at = |p| topology.node_at(p);
    let selected: Vec<_> = selection
        .iter()
        .map(|e| {
            drawing
                .elements
                .get(e.element_index)
                .filter(|element| element.endpoint_can_snap(e.is_start))
                .and_then(|_| endpoint_position(drawing, e))
                .and_then(node_at)
        })
        .collect();
    let target = selected
        .first()
        .copied()
        .flatten()
        .ok_or("Select one network node with the selection tool.")?;
    if selected.iter().any(|n| *n != Some(target)) {
        return Err("Select only one node.".into());
    }
    let edges: Vec<_> = drawing
        .elements
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Element::Resistor(a, b) => Some((i, node_at(*a).unwrap(), node_at(*b).unwrap())),
            _ => None,
        })
        .collect();
    let component = connected_component(target, &edges);
    let known: Vec<_> = component
        .iter()
        .map(|&n| boundary_pressure(drawing, &topology, n).map(|p| (n, p)))
        .collect::<Result<_, _>>()?;
    if !known.iter().any(|(_, p)| p.is_some()) {
        return Err("Network needs at least one labeled pressure boundary.".into());
    }
    // Validate every resistor and flow in this connected component, including known nodes.
    for (i, a, _) in &edges {
        if component.contains(a) {
            required_symbol(drawing.labels.get(*i), "each conductance")?;
        }
    }
    for (i, e) in drawing.elements.iter().enumerate() {
        if let Element::PrescribedFlow(_, p) = e {
            if node_at(*p).is_some_and(|n| component.contains(&n)) {
                required_symbol(drawing.labels.get(i), "each prescribed flow")?;
            }
        }
    }
    let target_name = drawing
        .node_labels
        .iter()
        .chain(&drawing.terminal_labels)
        .map(|l| (l.position, l.text.trim()))
        .find(|(position, text)| node_at(*position) == Some(target) && !text.is_empty())
        .map(|(_, text)| text.to_string())
        .unwrap_or_else(|| "p_selected".into());
    if let Some((_, Some(value))) = known.iter().find(|(n, _)| *n == target) {
        return Ok(format!("{target_name} = {}", value.text()));
    }
    let unknown: Vec<_> = known
        .iter()
        .filter(|(_, p)| p.is_none())
        .map(|(n, _)| *n)
        .collect();
    if unknown.len() > MAX_UNKNOWNS {
        return Err(format!(
            "This network has {} unknown nodes; the limit is {MAX_UNKNOWNS}. Reduce the network first.",
            unknown.len()
        ));
    }
    let target_index = unknown.iter().position(|n| *n == target).unwrap();
    if let Some(expression) =
        reference_solution(drawing, &topology, &edges, &known, &unknown, target_index)?
    {
        return Ok(format!("{target_name} = {expression}"));
    }
    let (matrix, rhs) = build_equations(drawing, &topology, &edges, &known, &unknown)?;
    let mut solutions = symbolic::compact_solutions(matrix, vec![rhs], target_index)?;
    Ok(format!("{target_name} = {}", solutions.remove(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::label::NodeLabel;
    fn p(x: f32) -> Pos2 {
        Pos2::new(x, 0.0)
    }
    fn chain() -> Drawing {
        let mut d = Drawing::empty();
        d.elements = vec![
            Element::Resistor(p(0.0), p(1.0)),
            Element::Resistor(p(1.0), p(2.0)),
        ];
        d.labels = vec!["G_1".into(), "G_2".into()];
        d.terminal_labels = vec![
            NodeLabel {
                position: p(0.0),
                text: "P_a".into(),
            },
            NodeLabel {
                position: p(2.0),
                text: "P_b".into(),
            },
        ];
        d
    }
    fn selected() -> Vec<DraggedEndpoint> {
        vec![DraggedEndpoint {
            element_index: 0,
            is_start: false,
        }]
    }
    #[test]
    fn supplied_series_network_simplifies() {
        let drawing = Drawing::from_json(include_str!(
            "../tests/fixtures/series_conductances.netdraw"
        ))
        .unwrap();
        let selection = [DraggedEndpoint {
            element_index: 0,
            is_start: false,
        }];
        let result = solve_selected(&drawing, &selection).unwrap();
        assert_eq!(result, "T = (K_1*T_3 + 2*K_2*T_1) / (K_1 + 2*K_2)");
        let other_node = [DraggedEndpoint {
            element_index: 0,
            is_start: true,
        }];
        assert_eq!(
            solve_selected(&drawing, &other_node).unwrap(),
            "p_selected = (K_1*T_3 + K_2*T_1 + K_2*T_3) / (K_1 + 2*K_2)"
        );
    }

    #[test]
    fn shared_boundaries_use_a_reference_and_factored_coefficient() {
        let drawing = Drawing::from_json(include_str!(
            "../tests/fixtures/shared_boundary_conductances.netdraw"
        ))
        .unwrap();
        let selection = [DraggedEndpoint {
            element_index: 0,
            is_start: false,
        }];
        let result = solve_selected(&drawing, &selection).unwrap();
        assert!(result.starts_with("T = T_0 + "));
        assert!(result.ends_with("*(T_e - T_0)"));
        assert!(result.len() < 230, "{result}");
    }

    #[test]
    fn shared_reference_preserves_independent_and_repeated_fluxes() {
        let mut drawing = chain();
        drawing.terminal_labels[0].text = "T_0".into();
        drawing.terminal_labels[1].text = "T_0".into();
        assert_eq!(
            solve_selected(&drawing, &selected()).unwrap(),
            "p_selected = T_0"
        );
        drawing
            .elements
            .push(Element::PrescribedFlow(Pos2::new(1.0, 1.0), p(1.0)));
        drawing.labels.push("Q".into());
        assert_eq!(
            solve_selected(&drawing, &selected()).unwrap(),
            "p_selected = T_0 + ((1) / (G_1 + G_2))*Q"
        );
        drawing
            .elements
            .push(Element::PrescribedFlow(Pos2::new(1.0, -1.0), p(1.0)));
        drawing.labels.push("Q".into());
        assert_eq!(
            solve_selected(&drawing, &selected()).unwrap(),
            "p_selected = T_0 + ((2) / (G_1 + G_2))*Q"
        );
    }

    #[test]
    fn pressure_divider() {
        let result = solve_selected(&chain(), &selected()).unwrap();
        assert_eq!(result, "p_selected = (G_1*P_a + G_2*P_b) / (G_1 + G_2)");
    }
    #[test]
    fn prescribed_flow_is_positive_injection() {
        let mut d = chain();
        d.elements
            .push(Element::PrescribedFlow(Pos2::new(1.0, 1.0), p(1.0)));
        d.labels.push("Q".into());
        let result = solve_selected(&d, &selected()).unwrap();
        assert_eq!(result, "p_selected = (G_1*P_a + G_2*P_b + Q) / (G_1 + G_2)");
    }
    #[test]
    fn prescribed_flux_is_a_node_independent_source() {
        let mut drawing = Drawing::empty();
        drawing.elements = vec![
            Element::Resistor(p(0.0), p(1.0)),
            Element::PrescribedFlow(Pos2::new(1.0, 1.0), p(1.0)),
        ];
        drawing.labels = vec!["G".into(), "Q".into()];
        drawing.terminal_labels = vec![NodeLabel {
            position: p(0.0),
            text: "P_boundary".into(),
        }];
        let topology = drawing_topology(&drawing);
        let boundary = topology.node_at(p(0.0)).unwrap();
        let node = topology.node_at(p(1.0)).unwrap();
        let known = vec![
            (boundary, Some(Expression::symbol("P_boundary"))),
            (node, None),
        ];
        let (matrix, rhs) =
            build_equations(&drawing, &topology, &[(0, boundary, node)], &known, &[node]).unwrap();
        // G * p = G * P_boundary + Q: Q never enters the node coefficient.
        assert_eq!(matrix[0][0].text(), "G");
        assert_eq!(rhs[0].text(), "G*P_boundary + Q");
        assert_eq!(
            solve_selected(&drawing, &selected()).unwrap(),
            "p_selected = (G*P_boundary + Q) / (G)"
        );
    }

    #[test]
    fn validates_labels_and_selection() {
        let mut d = chain();
        d.labels[1].clear();
        assert!(
            solve_selected(&d, &selected())
                .unwrap_err()
                .contains("conductance")
        );
        d = chain();
        d.terminal_labels.pop();
        assert!(
            solve_selected(&d, &selected())
                .unwrap_err()
                .contains("terminal")
        );
        assert!(solve_selected(&chain(), &[]).is_err());
        let mut endpoints = selected();
        endpoints.push(DraggedEndpoint {
            element_index: 0,
            is_start: true,
        });
        assert!(
            solve_selected(&chain(), &endpoints)
                .unwrap_err()
                .contains("only one")
        );
    }
    #[test]
    fn empty_terminal_label_is_not_a_boundary() {
        let mut d = chain();
        d.terminal_labels[1].text = "  ".into();
        assert!(
            solve_selected(&d, &selected())
                .unwrap_err()
                .contains("free terminal of conductance G_2")
        );
        // An empty label left at an internal node does not block solving.
        let mut d = chain();
        d.terminal_labels.push(NodeLabel {
            position: p(1.0),
            text: String::new(),
        });
        assert!(solve_selected(&d, &selected()).is_ok());
    }
    #[test]
    fn arrow_tail_does_not_connect_a_terminal() {
        let mut d = chain();
        d.terminal_labels.pop();
        d.elements
            .push(Element::PrescribedFlow(p(2.0), Pos2::new(2.0, 5.0)));
        d.labels.push("Q".into());
        // The renderer ticks this endpoint, and the solver asks for its label.
        assert!(unconnected_resistor_terminal_positions(&d.elements).contains(&p(2.0)));
        assert!(
            solve_selected(&d, &selected())
                .unwrap_err()
                .contains("free terminal")
        );
    }
    #[test]
    fn target_label_is_trimmed() {
        let mut d = chain();
        d.node_labels.push(NodeLabel {
            position: p(1.0),
            text: " T ".into(),
        });
        assert!(solve_selected(&d, &selected()).unwrap().starts_with("T = "));
    }
    #[test]
    fn ignores_disconnected_unlabeled_network() {
        let mut d = chain();
        d.elements.push(Element::Resistor(p(10.0), p(11.0)));
        assert!(solve_selected(&d, &selected()).is_ok());
    }
    #[test]
    fn enforces_unknown_node_limit() {
        let mut d = Drawing::empty();
        d.elements = (0..8)
            .map(|i| Element::Resistor(p(i as f32), p((i + 1) as f32)))
            .collect();
        d.labels = (0..8).map(|i| format!("G_{i}")).collect();
        d.terminal_labels = vec![
            NodeLabel {
                position: p(0.0),
                text: "A".into(),
            },
            NodeLabel {
                position: p(8.0),
                text: "B".into(),
            },
        ];
        assert!(
            solve_selected(&d, &selected())
                .unwrap_err()
                .contains("limit is 6")
        );
    }
    /// Six unknowns, four boundaries (one repeated), two flows.
    fn ladder(shared_boundary: bool) -> Drawing {
        let u = |i: usize| Pos2::new((i % 3) as f32 * 10.0, (i / 3) as f32 * 10.0);
        let mut d = Drawing::empty();
        for (a, b) in [(0, 1), (1, 2), (3, 4), (4, 5), (0, 3), (1, 4), (2, 5)] {
            d.elements.push(Element::Resistor(u(a), u(b)));
            d.labels.push(format!("G_{a}{b}"));
        }
        let names = [
            "T_0",
            if shared_boundary { "T_0" } else { "T_2" },
            "T_e",
            "T_1",
        ];
        for (k, (node, name)) in [0, 2, 5, 3].into_iter().zip(names).enumerate() {
            let free = u(node) + eframe::egui::vec2(-3.0, -3.0 - k as f32);
            d.elements.push(Element::Resistor(u(node), free));
            d.labels.push(format!("K_{k}"));
            d.terminal_labels.push(NodeLabel {
                position: free,
                text: name.into(),
            });
        }
        for (node, name) in [(1, "Q_1"), (4, "Q_2")] {
            d.elements.push(Element::PrescribedFlow(
                u(node) + eframe::egui::vec2(1.0, 7.0),
                u(node),
            ));
            d.labels.push(name.into());
        }
        d
    }
    #[test]
    fn six_unknown_grid_solves() {
        // Ordinary symbolic elimination exceeded the expression size limit here.
        let selection = [DraggedEndpoint {
            element_index: 1,
            is_start: true,
        }];
        let result = solve_selected(&ladder(false), &selection).unwrap();
        assert!(result.starts_with("p_selected = "), "{result}");
        let result = solve_selected(&ladder(true), &selection).unwrap();
        assert!(result.starts_with("p_selected = T_0 + "), "{result}");
        for name in ["T_e", "T_1", "Q_1", "Q_2"] {
            assert!(result.contains(name), "{name} missing from {result}");
        }
    }
    #[test]
    fn rejects_numeric_labels() {
        for number in ["0", "1.5", "-2", "1e3"] {
            let mut d = chain();
            d.terminal_labels[1].text = number.into();
            let error = solve_selected(&d, &selected()).unwrap_err();
            assert!(error.contains("is a number"), "{error}");
        }
        let mut d = chain();
        d.labels[0] = "2".into();
        assert!(
            solve_selected(&d, &selected())
                .unwrap_err()
                .contains("is a number")
        );
        // Names that merely contain digits stay valid symbols.
        let mut d = chain();
        d.terminal_labels[1].text = "T0".into();
        assert!(solve_selected(&d, &selected()).is_ok());
    }
}
