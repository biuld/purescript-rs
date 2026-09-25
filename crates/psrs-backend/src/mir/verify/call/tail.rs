//! Tail-call terminator verification for MIR.

use super::super::util::{call_value_types_match, composite_at, mir_error, require_value};
use super::Signature;
use crate::BackendError;
use crate::mir::{Function, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType, HeapType, RefType};
use psrs_hir::SymbolId;
use std::collections::HashMap;

/// Verifies a direct tail call: the arguments match the callee signature and
/// the callee's result becomes the caller's result.
pub(crate) fn verify_return_call(
    function: &Function,
    callee: SymbolId,
    arguments: &[ValueId],
    span: psrs_span::TextRange,
    definitions: &HashMap<ValueId, ValueType>,
    signatures: &HashMap<SymbolId, Option<Signature>>,
) -> Result<(), Vec<BackendError>> {
    let Some(Some(signature)) = signatures.get(&callee) else {
        return Err(mir_error(
            span,
            "MIR tail call targets a function with no valid signature",
        ));
    };
    if arguments.len() != signature.parameters.len()
        || arguments
            .iter()
            .zip(&signature.parameters)
            .any(|(argument, expected)| {
                require_value(definitions, *argument, span)
                    .ok()
                    .is_none_or(|actual| !call_value_types_match(actual, *expected))
            })
    {
        return Err(mir_error(span, "MIR tail call argument has the wrong type"));
    }
    let Some(result) = signature.result else {
        return Err(mir_error(span, "MIR tail call target returns no value"));
    };
    if !call_value_types_match(result, function.result_type) {
        return Err(mir_error(
            span,
            "MIR tail call result differs from its caller result",
        ));
    }
    Ok(())
}

/// Verifies a tail call through a typed function reference.
pub(crate) fn verify_return_call_ref(
    function: &Function,
    callee: ValueId,
    arguments: &[ValueId],
    span: psrs_span::TextRange,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Some(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(index),
    })) = require_value(definitions, callee, span).ok()
    else {
        return Err(mir_error(
            span,
            "MIR tail call_ref target has the wrong type",
        ));
    };
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, index)
    else {
        return Err(mir_error(
            span,
            "MIR tail call_ref type is not a function type",
        ));
    };
    if arguments.len() != parameters.len()
        || arguments
            .iter()
            .zip(parameters)
            .any(|(argument, expected)| {
                require_value(definitions, *argument, span)
                    .ok()
                    .is_none_or(|actual| !argument_matches(actual, *expected, defined))
            })
    {
        return Err(mir_error(
            span,
            "MIR tail call_ref argument has the wrong type",
        ));
    }
    if results.len() != 1 || !call_value_types_match(results[0], function.result_type) {
        return Err(mir_error(
            span,
            "MIR tail call_ref result differs from its caller result",
        ));
    }
    Ok(())
}

/// Matches a tail-call argument to a callee parameter. A closure receiver is
/// declared `(ref struct)` in the function type but has a concrete closure
/// struct type at the call site, so a struct reference is accepted there.
fn argument_matches(actual: ValueType, expected: ValueType, defined: &[&DefinedType]) -> bool {
    if call_value_types_match(actual, expected) {
        return true;
    }
    matches!(
        (actual, expected),
        (
            ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(index),
            }),
            ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Struct,
            }),
        ) if matches!(composite_at(defined, index), Some(CompositeType::Struct(_)))
    )
}
