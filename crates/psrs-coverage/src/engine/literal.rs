use crate::pattern::Literal;

/// Equates the spellings the official linter's `missingCases` treats
/// identically: finite `Number` literals reduce to their shortest standard
/// rendering, zero keeps a single spelling, and every other scalar is its own
/// canonical form.
pub(crate) fn canonical_literal(literal: &Literal) -> Literal {
    match literal {
        Literal::Number(text) => text
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .map_or_else(
                || literal.clone(),
                |number| {
                    if number == 0.0 {
                        Literal::Number("0".into())
                    } else {
                        Literal::Number(number.to_string())
                    }
                },
            ),
        _ => literal.clone(),
    }
}
