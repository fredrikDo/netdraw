//! Converts drawing topology into equations. All algebra lives in `symbolic`.
use crate::{
    document::Drawing,
    element::{DraggedEndpoint, Element},
    symbolic::{self, Expression},
};
use eframe::egui::Pos2;
use std::collections::BTreeMap;
// Count mathematical unknowns after wire merging, not drawn endpoints or boundaries.
const MAX_UNKNOWNS: usize = 6;

fn endpoint_position(drawing: &Drawing, endpoint: &DraggedEndpoint) -> Option<Pos2> {
    drawing.elements.get(endpoint.element_index).map(|e| {
        let (a, b) = e.endpoints();
        if endpoint.is_start { a } else { b }
    })
}
/// Trim a literal symbol name; do not interpret numbers or label typography.
fn required_symbol(label: Option<&String>, description: &str) -> Result<Expression, String> {
    let name = label
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Add a label to {description}."))?;
    Ok(Expression::symbol(name))
}

type Edge = (usize, usize, usize);
type Equations = (Vec<Vec<Expression>>, Vec<Expression>);

/// Map canvas positions to representative node IDs. Merging can leave gaps in IDs.
struct Topology {
    positions: Vec<Pos2>,
    nodes: Vec<usize>,
}
impl Topology {
    fn node_at(&self, position: Pos2) -> Option<usize> {
        self.positions
            .iter()
            .position(|p| *p == position)
            .map(|i| self.nodes[i])
    }
}
/// Merge ideal-wire endpoints; arrow tails and unsplit crossings create no nodes.
fn drawing_topology(drawing: &Drawing) -> Topology {
    let mut positions = Vec::new();
    for e in &drawing.elements {
        let (a, b) = e.endpoints();
        let endpoints = if matches!(e, Element::PrescribedFlow(..)) {
            vec![b]
        } else {
            vec![a, b]
        };
        for p in endpoints {
            if !positions.contains(&p) {
                positions.push(p);
            }
        }
    }
    let index = |p: Pos2| positions.iter().position(|x| *x == p);
    let mut nodes: Vec<usize> = (0..positions.len()).collect();
    for e in &drawing.elements {
        if let Element::Line(a, b) = e {
            let from = nodes[index(*a).unwrap()];
            let to = nodes[index(*b).unwrap()];
            for node in &mut nodes {
                if *node == from {
                    *node = to;
                }
            }
        }
    }
    Topology { positions, nodes }
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
/// A degree-one node with prescribed inflow can be unknown rather than a boundary.
fn boundary_pressure(
    drawing: &Drawing,
    topology: &Topology,
    edges: &[Edge],
    node: usize,
) -> Result<Option<Expression>, String> {
    let labels: Vec<_> = drawing
        .terminal_labels
        .iter()
        .filter(|l| topology.node_at(l.position) == Some(node))
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
    let degree = edges
        .iter()
        .filter(|(_, a, b)| a != b && (*a == node || *b == node))
        .count();
    let has_flow = drawing
        .elements
        .iter()
        .any(|e| matches!(e, Element::PrescribedFlow(_,p) if topology.node_at(*p) == Some(node)));
    if degree == 1 && !has_flow {
        return Err("Label every free terminal with its known pressure.".into());
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
    let mut terms = vec![reference.clone()];
    for boundary in counts.keys().filter(|name| *name != reference) {
        let unit_boundaries = unit_boundary_values(known, Some(boundary));
        let (matrix, rhs) = build_equations_with_sources(
            drawing,
            topology,
            edges,
            &unit_boundaries,
            unknown,
            |_| Ok(Expression::integer(0)),
        )?;
        let coefficient = symbolic::compact_solution(matrix, rhs, target)?;
        if coefficient != "0" {
            terms.push(format!("({coefficient})*({boundary} - {reference})"));
        }
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
        let (matrix, rhs) = build_equations_with_sources(
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
        let coefficient = symbolic::compact_solution(matrix, rhs, target)?;
        if coefficient != "0" {
            terms.push(format!("({coefficient})*{flow}"));
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
        return Err("Select only one node (connected wire endpoints count as one node).".into());
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
        .map(|&n| boundary_pressure(drawing, &topology, &edges, n).map(|p| (n, p)))
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
    let target_name = drawing
        .node_labels
        .iter()
        .chain(&drawing.terminal_labels)
        .find(|l| node_at(l.position) == Some(target) && !l.text.trim().is_empty())
        .map(|l| l.text.clone())
        .unwrap_or_else(|| "p_selected".into());
    if let Some((_, Some(value))) = known.iter().find(|(n, _)| *n == target) {
        return Ok(format!("{target_name} = {}", value.text()));
    }
    let target_index = unknown.iter().position(|n| *n == target).unwrap();
    if let Some(expression) =
        reference_solution(drawing, &topology, &edges, &known, &unknown, target_index)?
    {
        return Ok(format!("{target_name} = {expression}"));
    }
    let (matrix, rhs) = build_equations(drawing, &topology, &edges, &known, &unknown)?;
    Ok(format!(
        "{target_name} = {}",
        symbolic::compact_solution(matrix, rhs, target_index)?
    ))
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
    fn wires_join_nodes_and_boundaries() {
        let mut d = chain();
        d.elements.push(Element::Line(p(2.0), p(3.0)));
        d.labels.push(String::new());
        d.terminal_labels[1].position = p(3.0);
        assert!(solve_selected(&d, &selected()).unwrap().contains("G_2*P_b"));
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
}
