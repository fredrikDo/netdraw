//! Exact rational functions and linear elimination, independent of drawing/UI code.
use crate::symbolic_format::{Formula, determinant};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

// Sorted symbols encode a monomial: [a, a, b] means a^2*b; [] means 1.
type Monomial = Vec<String>;
/// Sparse integer polynomial. Canonical operations remove zero coefficients.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Polynomial(BTreeMap<Monomial, BigInt>);
// Bound intermediate growth as well as final output; a node cap alone is insufficient.
const MAX_TERMS: usize = 2048;
// Bounds the work of one multiplication; results are still capped at MAX_TERMS.
const MAX_PRODUCT_PAIRS: usize = 1 << 20;
type AlgebraResult<T> = Result<T, String>;
impl Polynomial {
    fn constant(n: i32) -> Self {
        Self(if n == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(vec![], n.into())])
        })
    }
    fn symbol(name: &str) -> Self {
        Self(BTreeMap::from([(vec![name.into()], BigInt::one())]))
    }
    fn add(&self, other: &Self) -> AlgebraResult<Self> {
        self.add_bounded(other, MAX_TERMS)
    }
    fn add_bounded(&self, other: &Self, max_terms: usize) -> AlgebraResult<Self> {
        let mut terms = self.0.clone();
        for (m, c) in &other.0 {
            *terms.entry(m.clone()).or_default() += c;
        }
        terms.retain(|_, c| !c.is_zero());
        if terms.len() > max_terms {
            return Err("Expression is too large; reduce the network first.".into());
        }
        Ok(Self(terms))
    }
    fn neg(&self) -> Self {
        Self(self.0.iter().map(|(m, c)| (m.clone(), -c)).collect())
    }
    fn mul(&self, other: &Self) -> AlgebraResult<Self> {
        self.mul_bounded(other, MAX_TERMS)
    }
    fn mul_bounded(&self, other: &Self, max_terms: usize) -> AlgebraResult<Self> {
        if self.0.len().saturating_mul(other.0.len()) > MAX_PRODUCT_PAIRS {
            return Err("Expression is too large; reduce the network first.".into());
        }
        let mut terms = BTreeMap::<Monomial, BigInt>::new();
        for (a, x) in &self.0 {
            for (b, y) in &other.0 {
                let mut m = a.clone();
                m.extend(b.iter().cloned());
                m.sort();
                *terms.entry(m).or_default() += x * y;
            }
        }
        terms.retain(|_, c| !c.is_zero());
        if terms.len() > max_terms {
            return Err("Expression is too large; reduce the network first.".into());
        }
        Ok(Self(terms))
    }
    fn is_constant(&self) -> bool {
        self.0.keys().all(Vec::is_empty)
    }
    /// Exact multivariate long division over integer coefficients.
    /// A failed or oversized division simply leaves the original expression intact.
    fn exact_quotient(&self, divisor: &Self) -> Option<Self> {
        let (leading_monomial, leading_coefficient) = divisor.leading_term()?;
        if divisor.is_constant() {
            return self
                .0
                .iter()
                .map(|(m, c)| {
                    (c % leading_coefficient)
                        .is_zero()
                        .then(|| (m.clone(), c / leading_coefficient))
                })
                .collect::<Option<_>>()
                .map(Self);
        }
        // Key the remainder by (degree, monomial), the graded order used by
        // `leading_term`, so the leading term is always the last entry, and subtract
        // in place: rescanning or rebuilding the remainder each step is quadratic.
        let mut remainder: BTreeMap<(usize, Monomial), BigInt> = self
            .0
            .iter()
            .map(|(m, c)| ((m.len(), m.clone()), c.clone()))
            .collect();
        let mut quotient = BTreeMap::new();
        for _ in 0..MAX_TERMS {
            let Some(((_, monomial), coefficient)) = remainder.last_key_value() else {
                return Some(Self(quotient));
            };
            let factor = divide_monomial(monomial, leading_monomial)?;
            if !(coefficient % leading_coefficient).is_zero() {
                return None;
            }
            let scale = coefficient / leading_coefficient;
            for (m, c) in &divisor.0 {
                let mut product = m.clone();
                product.extend(factor.iter().cloned());
                product.sort();
                match remainder.entry((product.len(), product)) {
                    Entry::Occupied(mut entry) => {
                        *entry.get_mut() -= c * &scale;
                        if entry.get().is_zero() {
                            entry.remove();
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(-(c * &scale));
                    }
                }
            }
            quotient.insert(factor, scale);
            if remainder.len() > MAX_TERMS * 4 {
                return None;
            }
        }
        None
    }
    fn leading_term(&self) -> Option<(&Monomial, &BigInt)> {
        leading_term(&self.0)
    }
    /// Normalize coefficient content/sign for comparison of cancellation candidates.
    fn primitive_part(&self) -> Self {
        let content = coefficient_gcd(self.0.values());
        if content.is_zero() {
            return self.clone();
        }
        let sign = if self.0.values().next().is_some_and(Signed::is_negative) {
            -content
        } else {
            content
        };
        Self(self.0.iter().map(|(m, c)| (m.clone(), c / &sign)).collect())
    }
    /// Convert canonical expanded terms to a precedence-aware presentation tree.
    fn formula(&self) -> Formula {
        Formula::sum(self.0.iter().map(|(monomial, coefficient)| {
            let mut factors = vec![Formula::atom(coefficient.abs().to_string())];
            factors.extend(monomial.iter().map(Formula::atom));
            let term = Formula::product(factors);
            if coefficient.is_negative() {
                term.negative()
            } else {
                term
            }
        }))
    }
    fn text(&self) -> String {
        if self.0.is_empty() {
            return "0".into();
        }
        let mut result = String::new();
        for (m, c) in &self.0 {
            let negative = c.is_negative();
            if result.is_empty() {
                if negative {
                    result.push('-');
                }
            } else {
                result.push_str(if negative { " - " } else { " + " });
            }
            let magnitude = c.abs();
            if m.is_empty() || magnitude != BigInt::one() {
                result.push_str(&magnitude.to_string());
                if !m.is_empty() {
                    result.push('*');
                }
            }
            result.push_str(&m.join("*"));
        }
        result
    }
}
/// Use a graded monomial order so each long-division step reduces the remainder.
fn leading_term(terms: &BTreeMap<Monomial, BigInt>) -> Option<(&Monomial, &BigInt)> {
    terms
        .iter()
        .max_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
}

/// Subtract symbol multiplicities, failing if the divisor is not a monomial factor.
fn divide_monomial(dividend: &Monomial, divisor: &Monomial) -> Option<Monomial> {
    let mut remaining = dividend.clone();
    for symbol in divisor {
        let index = remaining.iter().position(|s| s == symbol)?;
        remaining.remove(index);
    }
    Some(remaining)
}

/// Nonnegative integer content shared by the supplied coefficients.
fn coefficient_gcd<'a>(coefficients: impl Iterator<Item = &'a BigInt>) -> BigInt {
    coefficients.fold(BigInt::zero(), |mut gcd, coefficient| {
        let mut b = coefficient.abs();
        while !b.is_zero() {
            let remainder = &gcd % &b;
            gcd = b;
            b = remainder;
        }
        gcd
    })
}

/// Keep polynomial factors encountered during elimination as cancellation candidates.
/// This is deliberately bounded and is not a general polynomial factorization algorithm.
fn cancellation_factors(expressions: &[&Expression]) -> Vec<Polynomial> {
    let mut factors = Vec::new();
    for expression in expressions {
        for polynomial in expression
            .factors
            .iter()
            .chain([&expression.numerator, &expression.denominator])
        {
            let factor = polynomial.primitive_part();
            if factor.0.len() > 1 && !factors.contains(&factor) && factors.len() < 64 {
                factors.push(factor);
            }
        }
    }
    factors
}

fn cancel_polynomial_factors(
    mut numerator: Polynomial,
    mut denominator: Polynomial,
    factors: &[Polynomial],
) -> (Polynomial, Polynomial) {
    for factor in factors {
        // A constant denominator has no polynomial factor; try it first because the
        // denominator is usually the smaller side and a failure there skips the numerator.
        while !denominator.is_constant()
            && let Some(d) = denominator.exact_quotient(factor)
            && let Some(n) = numerator.exact_quotient(factor)
        {
            numerator = n;
            denominator = d;
        }
    }
    (numerator, denominator)
}

/// Exact rational function with bounded provenance for optional cancellation.
/// Denominators must be nonzero; this representation does not track parameter domains.
#[derive(Clone, Debug)]
pub struct Expression {
    numerator: Polynomial,
    denominator: Polynomial,
    // Candidates encountered in arithmetic, not a complete factorization of the value.
    factors: Vec<Polynomial>,
}
impl Expression {
    /// Construct an exact integer, including the canonical zero representation.
    pub fn integer(n: i32) -> Self {
        Self {
            numerator: Polynomial::constant(n),
            denominator: Polynomial::constant(1),
            factors: Vec::new(),
        }
    }
    /// Construct a literal, case-sensitive symbol; this is not an expression parser.
    pub fn symbol(name: &str) -> Self {
        Self {
            numerator: Polynomial::symbol(name),
            denominator: Polynomial::constant(1),
            factors: Vec::new(),
        }
    }
    /// Cancel verified factors, common monomials/content, and normalize denominator sign.
    fn normalized(
        numerator: Polynomial,
        denominator: Polynomial,
        factors: Vec<Polynomial>,
    ) -> Self {
        if numerator.0.is_empty() {
            return Self::integer(0);
        }
        if numerator == denominator {
            return Self::integer(1);
        }
        let (numerator, denominator) = cancel_polynomial_factors(numerator, denominator, &factors);
        // Cancel factors shared by every monomial in both polynomials.
        let mut common = numerator.0.keys().next().cloned().unwrap_or_default();
        for m in numerator.0.keys().chain(denominator.0.keys()) {
            let mut available = m.clone();
            common.retain(|s| {
                if let Some(i) = available.iter().position(|x| x == s) {
                    available.remove(i);
                    true
                } else {
                    false
                }
            });
        }
        let cancel = |p: Polynomial| {
            Polynomial(
                p.0.into_iter()
                    .map(|(mut m, c)| {
                        for s in &common {
                            if let Some(i) = m.iter().position(|x| x == s) {
                                m.remove(i);
                            }
                        }
                        (m, c)
                    })
                    .collect(),
            )
        };
        let mut n = cancel(numerator);
        let mut d = cancel(denominator);
        let gcd = coefficient_gcd(n.0.values().chain(d.0.values()));
        if gcd > BigInt::one() {
            for c in n.0.values_mut().chain(d.0.values_mut()) {
                *c /= &gcd;
            }
        }
        if d.0.values().next().is_some_and(Signed::is_negative) {
            Self {
                numerator: n.neg(),
                denominator: d.neg(),
                factors,
            }
        } else {
            Self {
                numerator: n,
                denominator: d,
                factors,
            }
        }
    }
    /// Add exact rational functions; identical denominators avoid unnecessary products.
    pub fn add(&self, other: &Self) -> AlgebraResult<Self> {
        if self.denominator == other.denominator {
            return Ok(Self::normalized(
                self.numerator.add(&other.numerator)?,
                self.denominator.clone(),
                cancellation_factors(&[self, other]),
            ));
        }
        Ok(Self::normalized(
            self.numerator
                .mul(&other.denominator)?
                .add(&other.numerator.mul(&self.denominator)?)?,
            self.denominator.mul(&other.denominator)?,
            cancellation_factors(&[self, other]),
        ))
    }
    /// Subtract while retaining cancellation provenance from both operands.
    pub fn sub(&self, other: &Self) -> AlgebraResult<Self> {
        self.add(&Self {
            numerator: other.numerator.neg(),
            denominator: other.denominator.clone(),
            factors: other.factors.clone(),
        })
    }
    /// Multiply and normalize, returning an error if intermediate polynomials grow too large.
    pub fn mul(&self, other: &Self) -> AlgebraResult<Self> {
        Ok(Self::normalized(
            self.numerator.mul(&other.numerator)?,
            self.denominator.mul(&other.denominator)?,
            cancellation_factors(&[self, other]),
        ))
    }
    /// Render expanded polynomial notation, useful for stable symbol names and diagnostics.
    pub fn text(&self) -> String {
        if self.denominator == Polynomial::constant(1) {
            self.numerator.text()
        } else {
            format!(
                "({}) / ({})",
                self.numerator.text(),
                self.denominator.text()
            )
        }
    }
}
/// Solve `A*x = b` for one unknown and several right-hand sides, choosing per result
/// between the cancelled exact value and a factored determinant ratio.
pub fn compact_solutions(
    matrix: Vec<Vec<Expression>>,
    rhs: Vec<Vec<Expression>>,
    target: usize,
) -> AlgebraResult<Vec<String>> {
    Ok(compact_formulas(matrix, rhs, target)?
        .iter()
        .map(Formula::text)
        .collect())
}

/// Shares one elimination and one denominator determinant across all right-hand sides.
fn compact_formulas(
    matrix: Vec<Vec<Expression>>,
    rhs: Vec<Vec<Expression>>,
    target: usize,
) -> AlgebraResult<Vec<Formula>> {
    let values = solve_target(&matrix, &rhs, target)?;
    let denominator: Vec<Vec<_>> = matrix
        .iter()
        .map(|row| row.iter().map(|e| e.numerator.formula()).collect())
        .collect();
    let denominator_determinant = determinant(&denominator);
    Ok(values
        .iter()
        .zip(&rhs)
        .map(|(value, b)| {
            let compact = Formula::ratio(value.numerator.formula(), value.denominator.formula());
            let mut numerator = denominator.clone();
            for (row, b) in numerator.iter_mut().zip(b) {
                row[target] = b.numerator.formula();
            }
            let factored = Formula::ratio(determinant(&numerator), denominator_determinant.clone());
            if factored.text().len() < compact.text().len() {
                factored
            } else {
                compact
            }
        })
        .collect())
}

/// Fraction-free (Bareiss) elimination on `[A without target | A target | b_1 ... b_m]`.
/// Every intermediate entry is a minor of the input, so expressions stay as small as
/// the determinants themselves and every division is exact. The last row then holds
/// `det(A)` and, for each `b_r`, the Cramer numerator for the target unknown.
/// Entries must be polynomials (denominator 1), as nodal equations always are.
fn solve_target(
    matrix: &[Vec<Expression>],
    rhs: &[Vec<Expression>],
    target: usize,
) -> AlgebraResult<Vec<Expression>> {
    let n = matrix.len();
    if matrix.iter().any(|row| row.len() != n) || rhs.iter().any(|b| b.len() != n) {
        return Err("Invalid equation dimensions.".into());
    }
    if target >= n {
        return Err("Invalid target node index.".into());
    }
    let polynomial = |e: &Expression| {
        if e.denominator == Polynomial::constant(1) {
            Ok(e.numerator.clone())
        } else {
            Err(String::from("Equations must have polynomial coefficients."))
        }
    };
    let mut rows = (0..n)
        .map(|i| {
            let columns = (0..n).filter(|&j| j != target).chain([target]);
            columns
                .map(|j| polynomial(&matrix[i][j]))
                .chain(rhs.iter().map(|b| polynomial(&b[i])))
                .collect::<AlgebraResult<Vec<_>>>()
        })
        .collect::<AlgebraResult<Vec<_>>>()?;
    let mut candidates = Vec::new();
    for entry in rows.iter().flat_map(|row| &row[..n]) {
        let factor = entry.primitive_part();
        if factor.0.len() > 1 && !candidates.contains(&factor) {
            candidates.push(factor);
        }
    }
    let too_large = || String::from("Expression is too large; reduce the network first.");
    let mut previous_pivot = Polynomial::constant(1);
    for k in 0..n {
        let pivot = (k..n)
            .find(|&i| !rows[i][k].0.is_empty())
            .ok_or("Network has no unique solution. Add a known pressure boundary.")?;
        rows.swap(k, pivot);
        let (upper, lower) = rows.split_at_mut(k + 1);
        let pivot_row = &upper[k];
        for row in lower {
            for j in k + 1..row.len() {
                // The cross product may exceed the cap briefly; the exact division
                // brings it back to the size of a minor, which must fit.
                let limit = MAX_TERMS * 8;
                let cross = pivot_row[k]
                    .mul_bounded(&row[j], limit)?
                    .add_bounded(&row[k].mul_bounded(&pivot_row[j], limit)?.neg(), limit)?;
                row[j] = if k == 0 {
                    cross
                } else {
                    cross
                        .exact_quotient(&previous_pivot)
                        .ok_or_else(too_large)?
                };
                if row[j].0.len() > MAX_TERMS {
                    return Err(too_large());
                }
            }
            row[k] = Polynomial::constant(0);
        }
        previous_pivot = rows[k][k].clone();
        // Leading minors are the usual source of factors shared by numerator and denominator.
        let factor = previous_pivot.primitive_part();
        if factor.0.len() > 1 && !candidates.contains(&factor) {
            candidates.push(factor);
        }
    }
    let last = rows.pop().unwrap_or_default();
    let (determinant, numerators) = (&last[n - 1], &last[n..]);
    Ok(numerators
        .iter()
        .map(|numerator| {
            Expression::normalized(numerator.clone(), determinant.clone(), candidates.clone())
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Every unknown, via one fraction-free elimination per target.
    fn solve(a: Vec<Vec<Expression>>, b: Vec<Expression>) -> AlgebraResult<Vec<Expression>> {
        (0..b.len())
            .map(|target| Ok(solve_target(&a, &[b.clone()], target)?.remove(0)))
            .collect()
    }
    impl Expression {
        fn is_zero(&self) -> bool {
            self.numerator.0.is_empty()
        }
        fn div(&self, other: &Self) -> AlgebraResult<Self> {
            assert!(!other.is_zero());
            Ok(Self::normalized(
                self.numerator.mul(&other.denominator)?,
                self.denominator.mul(&other.numerator)?,
                cancellation_factors(&[self, other]),
            ))
        }
    }
    fn evaluate(expression: &Expression, symbols: &[(&str, f64)]) -> f64 {
        use num_traits::ToPrimitive;
        let polynomial = |p: &Polynomial| {
            p.0.iter()
                .map(|(monomial, coefficient)| {
                    monomial
                        .iter()
                        .fold(coefficient.to_f64().unwrap(), |value, name| {
                            value
                                * symbols
                                    .iter()
                                    .find(|(symbol, _)| *symbol == name)
                                    .unwrap()
                                    .1
                        })
                })
                .sum::<f64>()
        };
        polynomial(&expression.numerator) / polynomial(&expression.denominator)
    }
    #[test]
    fn symbolic_solution_satisfies_coupled_network_equations() {
        let e = Expression::integer;
        let g = Expression::symbol("g");
        let matrix = vec![
            vec![g.add(&e(2)).unwrap(), e(-1), e(-1)],
            vec![e(-1), e(3), e(-1)],
            vec![e(-1), e(-1), e(4)],
        ];
        let rhs = vec![Expression::symbol("P"), Expression::symbol("Q"), e(0)];
        let result = solve(matrix.clone(), rhs.clone()).unwrap();
        for parameters in [
            [("g", 1.0), ("P", 10.0), ("Q", 2.0)],
            [("g", 2.5), ("P", -3.0), ("Q", 0.25)],
        ] {
            for (row, expected) in matrix.iter().zip(&rhs) {
                let actual: f64 = row
                    .iter()
                    .zip(&result)
                    .map(|(a, x)| evaluate(a, &parameters) * evaluate(x, &parameters))
                    .sum();
                assert!((actual - evaluate(expected, &parameters)).abs() < 1e-9);
            }
        }
    }
    #[test]
    fn cancels_repeated_polynomial_factors_exactly() {
        let a = Expression::symbol("a");
        let b = Expression::symbol("b");
        let factor = a.add(&b).unwrap();
        let p = Expression::symbol("P");
        let squared = factor.mul(&factor).unwrap();
        let numerator = squared.mul(&p).unwrap();
        let denominator = squared.mul(&Expression::integer(3)).unwrap();
        let simplified = numerator.div(&denominator).unwrap();
        assert_eq!(simplified.text(), "(P) / (3)");
        assert_eq!(
            numerator.numerator.mul(&simplified.denominator).unwrap(),
            denominator.numerator.mul(&simplified.numerator).unwrap()
        );
    }
    #[test]
    fn exact_division_handles_multivariate_products_and_rejects_remainders() {
        let a = Polynomial::symbol("a");
        let b = Polynomial::symbol("b");
        let factor = a.add(&b).unwrap();
        let quotient = a.mul(&b).unwrap().add(&Polynomial::constant(-2)).unwrap();
        let product = factor.mul(&quotient).unwrap();
        assert_eq!(product.exact_quotient(&factor), Some(quotient));
        assert!(
            product
                .add(&Polynomial::constant(1))
                .unwrap()
                .exact_quotient(&factor)
                .is_none()
        );
        assert!(a.exact_quotient(&Polynomial::constant(2)).is_none());
        assert!(a.exact_quotient(&Polynomial::constant(0)).is_none());
    }
    #[test]
    fn keeps_genuine_denominators() {
        let a = Expression::symbol("a");
        let b = Expression::symbol("b");
        let sum = a.add(&b).unwrap();
        let fraction = sum
            .mul(&a)
            .unwrap()
            .add(&Expression::integer(1))
            .unwrap()
            .div(&sum)
            .unwrap();
        for parameters in [[("a", 2.0), ("b", 3.0)], [("a", -1.0), ("b", 4.0)]] {
            let av = parameters[0].1;
            let bv = parameters[1].1;
            assert!((evaluate(&fraction, &parameters) - (av + 1.0 / (av + bv))).abs() < 1e-12);
        }
        assert_ne!(fraction.denominator, Polynomial::constant(1));
    }
    #[test]
    fn factored_transfer_matches_exact_algebra_and_analytic_solution() {
        let ka = Expression::symbol("K_a");
        let kh = Expression::symbol("K_h");
        let kt = Expression::symbol("K_t");
        let kc = Expression::symbol("K_C");
        let diagonal = ka.add(&kh).unwrap().add(&kt).unwrap();
        let zero = Expression::integer(0);
        let matrix = vec![
            vec![kc.add(&kh).unwrap(), zero.sub(&kh).unwrap(), zero.clone()],
            vec![
                zero.sub(&kh).unwrap(),
                diagonal.clone(),
                zero.sub(&kt).unwrap(),
            ],
            vec![zero.clone(), zero.sub(&kt).unwrap(), diagonal],
        ];
        let rhs = vec![zero.clone(), zero, kh];
        let exact = solve(matrix.clone(), rhs.clone()).unwrap();
        let formula = compact_formulas(matrix, vec![rhs], 0).unwrap().remove(0);
        assert!(formula.text().contains("^2"));
        for parameters in [
            [("K_a", 1.0), ("K_h", 2.0), ("K_t", 3.0), ("K_C", 4.0)],
            [("K_a", 0.1), ("K_h", 8.0), ("K_t", 2.0), ("K_C", 0.3)],
            [("K_a", 3.0), ("K_h", 1.0), ("K_t", 0.2), ("K_C", 7.0)],
        ] {
            let symbol = |name: &str| parameters.iter().find(|(s, _)| *s == name).unwrap().1;
            let (ka, kh, kt, kc) = (symbol("K_a"), symbol("K_h"), symbol("K_t"), symbol("K_C"));
            let expected = kh * kh * kt
                / ((kh + kc) * (ka + kh) * (ka + kh + 2.0 * kt) - kh * kh * (ka + kh + kt));
            assert!((formula.evaluate(&symbol) - expected).abs() < 1e-12);
            assert!((evaluate(&exact[0], &parameters) - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn fraction_free_elimination_handles_late_zero_pivots() {
        // The leading 2x2 minor is a*b - b*a = 0, forcing a row swap at the second step.
        let e = Expression::integer;
        let (a, b, c, d) = (
            Expression::symbol("a"),
            Expression::symbol("b"),
            Expression::symbol("c"),
            Expression::symbol("d"),
        );
        let matrix = vec![
            vec![a.clone(), b.clone(), e(0), e(1)],
            vec![a.clone(), b.clone(), e(1), e(0)],
            vec![e(0), e(1), c.clone(), e(0)],
            vec![e(1), e(0), e(0), d.clone()],
        ];
        let rhs = vec![Expression::symbol("P"), e(0), Expression::symbol("Q"), e(2)];
        let result = solve(matrix.clone(), rhs.clone()).unwrap();
        for parameters in [
            [
                ("a", 2.0),
                ("b", 3.0),
                ("c", 5.0),
                ("d", 7.0),
                ("P", 1.0),
                ("Q", -4.0),
            ],
            [
                ("a", 0.5),
                ("b", -1.5),
                ("c", 2.0),
                ("d", 0.3),
                ("P", 9.0),
                ("Q", 2.0),
            ],
        ] {
            for (row, expected) in matrix.iter().zip(&rhs) {
                let actual: f64 = row
                    .iter()
                    .zip(&result)
                    .map(|(a, x)| evaluate(a, &parameters) * evaluate(x, &parameters))
                    .sum();
                assert!((actual - evaluate(expected, &parameters)).abs() < 1e-9);
            }
        }
    }
    #[test]
    fn swaps_zero_pivot_rows() {
        let e = Expression::integer;
        let result = solve(vec![vec![e(0), e(1)], vec![e(1), e(1)]], vec![e(2), e(5)]).unwrap();
        assert_eq!(result[0].text(), "3");
        assert_eq!(result[1].text(), "2");
    }
    #[test]
    fn solves_coupled_equations() {
        let e = Expression::integer;
        let result = solve(
            vec![vec![e(2), e(-1)], vec![e(-1), e(2)]],
            vec![Expression::symbol("P"), e(0)],
        )
        .unwrap();
        assert_eq!(result[1].text(), "(P) / (3)");
    }
    #[test]
    fn detects_singular_network() {
        let e = Expression::integer;
        assert!(solve(vec![vec![e(1), e(-1)], vec![e(-1), e(1)]], vec![e(0), e(0)]).is_err());
    }
    #[test]
    fn cancels_and_combines_terms() {
        let r = Expression::symbol("R");
        let p = Expression::symbol("P");
        assert_eq!(r.mul(&p).unwrap().div(&r).unwrap().text(), "P");
        assert!(p.sub(&p).unwrap().is_zero());
    }
}
