# Solving network nodes

NetDraw solves steady, linear conductance networks symbolically. A node value can
represent temperature, pressure, or another potential. Use consistent physical
units throughout a drawing. No Python installation or external algebra service is
required.

## Prepare and solve a drawing

1. Draw the conductances with the zigzag tool. Join endpoints by snapping them
   together. Plain lines connect their endpoints with an ideal connection.
2. Give every zigzag a conductance label, such as `K_h` or `G_1`.
3. Label every free pressure/temperature terminal with its known boundary value,
   such as `T_0`, `T_e`, or `P_in`.
4. If the network contains prescribed-flow arrows, label each arrow with its
   prescribed flux, such as `Q`. Only the arrow tip connects to the network.
5. Select one node with the selection tool. Selecting several endpoints is allowed
   if they all belong to that same mathematical node, including endpoints joined
   by plain lines.
6. Click **Solve selected node** (`=`). Read or copy the expression in the result
   window.

An internal node label names the result; it does not prescribe that node's value.
If the selected node has no label, the result is named `p_selected`. Selecting a
known boundary returns its known value.

Results are snapshots. After changing geometry or labels, click Solve again.
Editing the displayed result text does not change the drawing or the solver.

## Meaning of labels and connections

| Drawing item | Meaning during solving |
| --- | --- |
| Zigzag labeled `G` | Conductance: flux from node `i` to `j` is `G*(p_i - p_j)` |
| Free terminal label | Known node value, or boundary condition |
| Internal node label | Name used when displaying that node's result |
| Arrow labeled `Q` | Node-independent positive flux injection at its tip |

Despite the internal Rust name `Resistor` and the saved type `resistor`, zigzag
labels represent **conductance**, not resistance. Existing files retain their
labels; a label named `R_1` is still treated as a conductance symbol by the solver.

Connectivity uses coincident endpoints, not visual proximity. Snapping makes the
coordinates coincide. Crossing lines and endpoints touching the middle of an
unsplit element do not create a connection. A flow arrow's tail is decorative.

Labels are literal symbols after trimming surrounding whitespace. Reusing `K_a`
means the same conductance in every occurrence; reusing `T_0` means the same
boundary value. `G_1` and `g_1` are different symbols. Label typography does not
parse algebra: `2*G` and `-Q` are symbol names, not a product or a negative source.
Labels that are plain numbers, such as `0`, `1.5`, or `-2`, are rejected so a symbol
can never be confused with a numerical value. Prefer simple names such as `K_h`, `T_0`, and `Q_in` for
readable output. Numerical substitution and expression input are not implemented.

## Equations and examples

At each unknown node, the solver applies conservation:

```text
sum(G_ij * (p_i - p_j)) = Q_i
```

`Q_i` is the sum of prescribed inflows at that node. It is independent of node
values. Known boundary values move to the right-hand side of the linear system.
A prescribed inflow on a known boundary does not change the unknown node values;
the prescribed boundary value remains fixed.

For a single conductance to a known boundary with prescribed inflow:

```text
G * (p - p_boundary) = Q
p = p_boundary + Q / G
```

For an unknown node joined to boundaries `P_a` and `P_b`:

```text
p = (G_1*P_a + G_2*P_b + Q) / (G_1 + G_2)
```

### Shared boundary values

When several mathematical boundary nodes have the same label, the most frequent
shared value becomes a reference. The solver computes independent responses to
boundary differences and prescribed fluxes. For example:

```text
T = T_0 + (conductance coefficient)*(T_e - T_0)
```

The coefficient is written directly in the result; the solver does not introduce
an extra `alpha` symbol. If all boundaries are `T_0` and there are no prescribed
inflows, the solution is simply `T_0`.

Sums and products may stay factored, and repeated factors may use powers such as
`K_h^2`. The formatter chooses the shorter of a cancelled polynomial form and a
factored determinant form for each response. It does not guarantee the shortest
possible algebraic expression.

## Limits and validation

A free terminal is a zigzag endpoint shared with no other zigzag endpoint or arrow
tip; arrow tails never connect. The canvas marks each one with a terminal tick, and
each one needs a non-empty label. Empty labels are ignored.


- At most **six unknown mathematical nodes** in the selected connected network.
  Known boundary nodes do not count. Disconnected parts of the drawing are ignored.
- At least one labeled known boundary is required. Label all conductances,
  prescribed fluxes, and free boundaries in the selected component.
- Connected boundary terminals must agree on their label.
- Conductances are assumed positive and finite. This is a modeling assumption,
  not something the symbolic labels can enforce.
- Polynomial size and simplification work are bounded. A network can reach an
  expression-size limit before reaching the node limit.
- The solver handles steady linear networks. Nonlinear elements, transient
  storage, numerical evaluation, and interactive network reduction are not yet
  implemented.

If Solve reports missing labels, add them to the indicated element type and try
again. If it reports a non-unique solution, check boundary conditions and
connectivity. If it reports the node or expression-size limit, use a smaller model;
a built-in reduction workflow is planned but is not available yet.

See [the implementation guide](architecture.md#symbolic-solving-pipeline) for the
algorithms, invariants, and current simplification limits.
