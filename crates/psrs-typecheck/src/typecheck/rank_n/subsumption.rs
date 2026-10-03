use super::super::unify::substitute;
use super::super::*;

impl Checker {
    /// Checks semantic subtyping for the supported rank-N fragment. A source
    /// forall may be instantiated; an expected forall is checked with rigid
    /// binders; function parameters are contravariant and results covariant.
    pub(in crate::typecheck) fn subsume(
        &mut self,
        actual: InferType,
        expected: InferType,
        span: TextRange,
    ) {
        let actual = self.resolve_type(actual);
        let expected = self.resolve_type(expected);
        match (actual, expected) {
            (actual, InferType::ForAll { variables, body }) => {
                self.with_skolem_scope(&variables, |checker| checker.subsume(actual, *body, span));
            }
            (InferType::ForAll { variables, body }, expected) => {
                let mapping = self.instantiate_type_variables(variables);
                self.subsume(substitute(&body, &mapping), expected, span);
            }
            (actual, expected) => {
                if let (Some(actual_row), Some(expected_row)) =
                    (record_row(&actual), record_row(&expected))
                {
                    let actual = self.normalize_row_or_report(actual_row, span);
                    let expected = self.normalize_row_or_report(expected_row, span);
                    let actual_fields = actual.fields.into_iter().collect::<HashMap<_, _>>();
                    let expected_fields = expected.fields.into_iter().collect::<HashMap<_, _>>();
                    let mut actual_remainder = Vec::new();
                    let mut expected_remainder = Vec::new();
                    for (label, actual_field) in &actual_fields {
                        if let Some(expected_field) = expected_fields.get(label) {
                            self.subsume(actual_field.clone(), expected_field.clone(), span);
                        } else {
                            actual_remainder.push((label.clone(), actual_field.clone()));
                        }
                    }
                    for (label, expected_field) in &expected_fields {
                        if !actual_fields.contains_key(label) {
                            expected_remainder.push((label.clone(), expected_field.clone()));
                        }
                    }
                    self.unify_rows(
                        row_from_fields(actual_remainder, actual.tail.to_type()),
                        row_from_fields(expected_remainder, expected.tail.to_type()),
                        span,
                    );
                    return;
                }
                if let (
                    InferType::Application(actual_inner, actual_result),
                    InferType::Application(expected_inner, expected_result),
                ) = (&actual, &expected)
                    && let (
                        Some((actual_parameter, actual_result)),
                        Some((expected_parameter, expected_result)),
                    ) = (
                        infer_arrow_parts(actual_inner, actual_result),
                        infer_arrow_parts(expected_inner, expected_result),
                    )
                {
                    self.subsume(expected_parameter, actual_parameter, span);
                    self.subsume(actual_result, expected_result, span);
                } else {
                    // `unify` reports its first argument as the expected type.
                    self.unify(expected, actual, span);
                }
            }
        }
    }
}
