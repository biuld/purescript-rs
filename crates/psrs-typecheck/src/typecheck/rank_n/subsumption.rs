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
            (
                actual,
                InferType::ForAll {
                    variables, body, ..
                },
            ) => {
                self.with_skolem_scope(&variables, |checker| checker.subsume(actual, *body, span));
            }
            (
                InferType::ForAll {
                    variables, body, ..
                },
                expected,
            ) => {
                let mapping = self.instantiate_type_variables(variables, &HashMap::new());
                self.subsume(substitute(&body, &mapping), expected, span);
            }
            (actual, expected) => {
                if let (Some(actual_row), Some(expected_row)) =
                    (record_row(&actual), record_row(&expected))
                {
                    let actual = self.normalize_row_or_report(actual_row, span);
                    let expected = self.normalize_row_or_report(expected_row, span);
                    // `{ ts1 | r1 }` subsumes `{ ts2 | r2 }`. Duplicate labels
                    // stay in the alignment: a map would drop the extra
                    // occurrence and solve the tail as an empty row.
                    // A closed candidate that lacks an expected label is
                    // `PropertyIsMissing`; a closed expected row that cannot
                    // accept an extra label is `AdditionalProperty`. Those
                    // reports stop the check, matching `throwError` before
                    // `unifyTypes`. Every other mismatch is ordinary row
                    // unification of the leftovers.
                    let (common, actual_remainder, expected_remainder) =
                        align_row_fields(actual.fields, expected.fields);
                    let reported = report_closed_record_properties(
                        self,
                        &actual_remainder,
                        matches!(actual.tail.to_type(), InferType::RowEmpty),
                        &expected_remainder,
                        matches!(expected.tail.to_type(), InferType::RowEmpty),
                        span,
                    );
                    if reported {
                        return;
                    }
                    for (actual_field, expected_field) in common {
                        self.subsume(actual_field, expected_field, span);
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

/// Reports at most one missing label and one additional label, matching
/// `subsumes'` in `TypeChecker/Subsumption.hs`, which keeps the first
/// difference on each side.
fn report_closed_record_properties(
    checker: &mut Checker,
    actual_remainder: &[(String, InferType)],
    actual_closed: bool,
    expected_remainder: &[(String, InferType)],
    expected_closed: bool,
    span: TextRange,
) -> bool {
    let mut reported = false;
    if actual_closed && let Some(label) = first_label(expected_remainder) {
        checker.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::PropertyIsMissing,
            span,
            format!("type of expression lacks required label `{label}`"),
        ));
        reported = true;
    }
    if expected_closed && let Some(label) = first_label(actual_remainder) {
        checker.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::AdditionalProperty,
            span,
            format!("type of expression contains additional label `{label}`"),
        ));
        reported = true;
    }
    reported
}

fn first_label(fields: &[(String, InferType)]) -> Option<&str> {
    // `align_row_fields` leaves each remainder sorted, and `firstMissingProp`
    // reports the first label of that sorted difference.
    fields.first().map(|(label, _)| label.as_str())
}
