//! Factored expression trees used only for presentation; exact arithmetic stays in symbolic.rs.

/// Presentation-only tree: preserves grouping without changing exact arithmetic.
/// Construct through the smart constructors (`sum`, `product`, ...), which simplify
/// trivial cases; the literal atoms `"0"` and `"1"` act as zero and one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Formula {
    Atom(String),
    Sum(Vec<Formula>),
    Product(Vec<Formula>),
    Negative(Box<Formula>),
    Power(Box<Formula>, usize),
    Ratio(Box<Formula>, Box<Formula>),
}
impl Formula {
    /// A symbol name or an integer literal.
    pub fn atom(value: impl Into<String>) -> Self {
        Self::Atom(value.into())
    }
    /// Structural zero test; only the literal atom `0` counts.
    pub fn is_zero(&self) -> bool {
        *self == Self::atom("0")
    }
    /// Negate, cancelling double negation and leaving zero unsigned.
    pub fn negative(self) -> Self {
        match self {
            Self::Negative(value) => *value,
            value if value.is_zero() => value,
            value => Self::Negative(Box::new(value)),
        }
    }
    /// Drop zero terms while retaining nested grouping from determinant expansion.
    pub fn sum(terms: impl IntoIterator<Item = Self>) -> Self {
        let terms: Vec<_> = terms.into_iter().filter(|t| !t.is_zero()).collect();
        match terms.len() {
            0 => Self::atom("0"),
            1 => terms.into_iter().next().unwrap(),
            _ => Self::Sum(terms),
        }
    }
    /// Remove identities, extract signs, and group repeated factors into powers.
    pub fn product(terms: impl IntoIterator<Item = Self>) -> Self {
        let mut factors = Vec::new();
        let mut negative = false;
        for term in terms {
            if term.is_zero() {
                return Self::atom("0");
            }
            let term = match term {
                Self::Negative(value) => {
                    negative = !negative;
                    *value
                }
                value => value,
            };
            if term == Self::atom("1") {
                continue;
            }
            match term {
                Self::Product(values) => factors.extend(values),
                value => factors.push(value),
            }
        }
        let mut grouped: Vec<(Self, usize)> = Vec::new();
        for factor in factors {
            if let Some((_, count)) = grouped.iter_mut().find(|(value, _)| *value == factor) {
                *count += 1;
            } else {
                grouped.push((factor, 1));
            }
        }
        let factors: Vec<_> = grouped
            .into_iter()
            .map(|(value, count)| {
                if count == 1 {
                    value
                } else {
                    Self::Power(Box::new(value), count)
                }
            })
            .collect();
        let result = match factors.len() {
            0 => Self::atom("1"),
            1 => factors.into_iter().next().unwrap(),
            _ => Self::Product(factors),
        };
        if negative { result.negative() } else { result }
    }
    /// Handle trivial ratios; nontrivial polynomial cancellation belongs to the algebra.
    pub fn ratio(numerator: Self, denominator: Self) -> Self {
        if numerator.is_zero() {
            numerator
        } else if denominator == Self::atom("1") {
            numerator
        } else if numerator == denominator {
            Self::atom("1")
        } else {
            Self::Ratio(Box::new(numerator), Box::new(denominator))
        }
    }
    #[cfg(test)]
    /// Test oracle for tree semantics, independent of rendered text.
    pub fn evaluate(&self, symbol: &impl Fn(&str) -> f64) -> f64 {
        match self {
            Self::Atom(value) => value.parse().unwrap_or_else(|_| symbol(value)),
            Self::Sum(terms) => terms.iter().map(|v| v.evaluate(symbol)).sum(),
            Self::Product(terms) => terms.iter().map(|v| v.evaluate(symbol)).product(),
            Self::Negative(value) => -value.evaluate(symbol),
            Self::Power(value, n) => value.evaluate(symbol).powi(*n as i32),
            Self::Ratio(n, d) => n.evaluate(symbol) / d.evaluate(symbol),
        }
    }
    /// Larger numbers bind more tightly; nested formatting protects lower-precedence children.
    fn precedence(&self) -> u8 {
        match self {
            Self::Sum(_) => 1,
            Self::Product(_) | Self::Ratio(..) => 2,
            Self::Negative(_) => 3,
            Self::Power(..) => 4,
            Self::Atom(_) => 5,
        }
    }
    /// Text of `self` as an operand of an operator with the given precedence.
    fn nested(&self, precedence: u8) -> String {
        let text = self.text();
        if self.precedence() < precedence {
            format!("({text})")
        } else {
            text
        }
    }
    /// Render explicit products and parenthesized fractions with unambiguous grouping.
    pub fn text(&self) -> String {
        match self {
            Self::Atom(value) => value.clone(),
            Self::Negative(value) => format!("-{}", value.nested(3)),
            Self::Sum(terms) => {
                let mut text = String::new();
                for term in terms {
                    if text.is_empty() {
                        text = term.text();
                    } else if let Self::Negative(value) = term {
                        text.push_str(&format!(" - {}", value.nested(2)));
                    } else {
                        text.push_str(&format!(" + {}", term.text()));
                    }
                }
                text
            }
            Self::Product(factors) => factors
                .iter()
                .map(|f| f.nested(3))
                .collect::<Vec<_>>()
                .join("*"),
            Self::Power(value, count) => format!("{}^{count}", value.nested(4)),
            Self::Ratio(n, d) => format!("({}) / ({})", n.text(), d.text()),
        }
    }
}

/// Sparse Laplace expansion preserves sums/products from the nodal matrix. It expands
/// along the row with the fewest nonzero entries; the cost is factorial in the worst
/// case, which the solver's six-unknown limit keeps small.
pub fn determinant(matrix: &[Vec<Formula>]) -> Formula {
    let n = matrix.len();
    if n == 0 {
        return Formula::atom("1");
    }
    if n == 1 {
        return matrix[0][0].clone();
    }
    let row = (0..n)
        .min_by_key(|&r| matrix[r].iter().filter(|v| !v.is_zero()).count())
        .unwrap();
    Formula::sum(
        (0..n)
            .filter(|&col| !matrix[row][col].is_zero())
            .map(|col| {
                let minor: Vec<Vec<_>> = matrix
                    .iter()
                    .enumerate()
                    .filter(|(r, _)| *r != row)
                    .map(|(_, values)| {
                        values
                            .iter()
                            .enumerate()
                            .filter(|(c, _)| *c != col)
                            .map(|(_, v)| v.clone())
                            .collect()
                    })
                    .collect();
                let term = Formula::product([matrix[row][col].clone(), determinant(&minor)]);
                if (row + col) % 2 == 0 {
                    term
                } else {
                    term.negative()
                }
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factored_text_preserves_precedence_and_powers() {
        let sum = Formula::sum([Formula::atom("a"), Formula::atom("b")]);
        let expression = Formula::ratio(Formula::product([sum.clone(), sum]), Formula::atom("c"));
        assert_eq!(expression.text(), "((a + b)^2) / (c)");
        assert_eq!(
            expression.evaluate(&|name| match name {
                "a" => 2.0,
                "b" => 3.0,
                "c" => 5.0,
                _ => unreachable!(),
            }),
            5.0
        );
    }
    #[test]
    fn sparse_determinants_preserve_cofactor_signs() {
        for (values, expected) in [
            (vec![vec![0, 2, 0], vec![3, 4, 0], vec![0, 0, 5]], -30.0),
            (vec![vec![1, 2, 3], vec![0, 0, 4], vec![5, 6, 7]], 16.0),
            (vec![vec![1, 2], vec![2, 4]], 0.0),
        ] {
            let matrix: Vec<Vec<_>> = values
                .iter()
                .map(|row| row.iter().map(|v| Formula::atom(v.to_string())).collect())
                .collect();
            assert_eq!(determinant(&matrix).evaluate(&|_| unreachable!()), expected);
        }
    }
}
