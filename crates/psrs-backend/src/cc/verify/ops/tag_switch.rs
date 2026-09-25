use super::super::helpers::{assignment_error, declared_shape, require_value_shape, undef_error};
use super::verify_assignments;
use crate::BackendError;
use crate::cc::{Assignment, Function, Signature, TagCase, ValueShape};
use crate::types::ValueId;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_tag_switch(
    assignment: &Assignment,
    value: ValueId,
    cases: &[TagCase],
    default_assignments: &[Assignment],
    default_value: ValueId,
    available: &HashSet<ValueId>,
    declared: &HashMap<ValueId, ValueShape>,
    signatures: &HashMap<SymbolId, Signature>,
    table: &crate::cc::RepresentationTable,
    functions: Option<&HashMap<SymbolId, &Function>>,
    function_span: TextRange,
) -> Result<(), Vec<BackendError>> {
    require_value_shape(declared, value, ValueShape::Integer, assignment)?;
    if cases.is_empty()
        || cases
            .iter()
            .map(|case| case.tag)
            .collect::<HashSet<_>>()
            .len()
            != cases.len()
    {
        return Err(assignment_error(
            assignment,
            "tag switch must have cases with unique tags",
        ));
    }

    let expected = declared_shape(declared, default_value, assignment)?;
    if expected != declared_shape(declared, assignment.destination, assignment)? {
        return Err(assignment_error(
            assignment,
            "tag switch default has an incompatible result shape",
        ));
    }
    let mut default_available = available.clone();
    verify_assignments(
        default_assignments,
        &mut default_available,
        declared,
        signatures,
        table,
        functions,
        function_span,
    )?;
    if !default_available.contains(&default_value) {
        return Err(undef_error(assignment.span, function_span));
    }

    for case in cases {
        let mut case_available = available.clone();
        verify_assignments(
            &case.assignments,
            &mut case_available,
            declared,
            signatures,
            table,
            functions,
            function_span,
        )?;
        if !case_available.contains(&case.value)
            || declared_shape(declared, case.value, assignment)? != expected
        {
            return Err(assignment_error(
                assignment,
                "tag switch cases have incompatible result shapes",
            ));
        }
    }
    Ok(())
}
