//! Source payload recovery is restricted to the machine's checked use roles.
use super::execution::View;
use super::traces::Trace;
use crate::cc::{
    AssignmentKind as A, Function, RefShape, Representation, RepresentationTable, ValueShape,
};
use crate::control::execution::Operands;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
    path: &Trace,
) -> Result<(), &'static str> {
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let raw = table
        .control_resumption
        .as_ref()
        .ok_or("refinement has no raw contract")?
        .function();
    let view = View {
        definitions: definitions.clone(),
        aliases: Some(&path.aliases),
        raw,
    };
    let next = match definitions.get(&view.origin(function.result)).copied() {
        Some(A::DirectCall {
            function: target,
            arguments,
            ..
        }) if *target == function.symbol => Some(arguments.as_slice()),
        _ => None,
    };
    for value in &path.casts {
        let Some(A::RepresentationCast {
            value, reference, ..
        }) = definitions.get(value).copied()
        else {
            return Err("refinement has no cast definition");
        };
        let source = view.origin(*value);
        let known = function
            .values
            .iter()
            .find(|v| v.id == source)
            .and_then(|v| match v.ty {
                ValueShape::Reference(reference) => Some(reference),
                _ => None,
            });
        if known.is_some_and(|actual| (reference.nullable || !actual.nullable) && (actual.heap == reference.heap || reference.heap == RefShape::Erased || (reference.heap == RefShape::Aggregate && matches!(actual.heap, RefShape::Repr(repr) if matches!(table.representation(repr), Some(Representation::Product { .. } | Representation::Variant { .. })))))) { continue; }
        if matches!(reference.heap, RefShape::Closure(_)) && path.events.iter().any(|event| matches!(event, crate::control::events::Event::Apply { result } if view.application(*result).is_some_and(|(callee, _)| view.origin(callee) == source))) { continue; }
        if let RefShape::Repr(repr) = reference.heap
            && next.is_some_and(|args| {
                args.len() == 6
                    && (table.control_checkpoint_schema().is_some_and(|schema| {
                        schema.computation == repr && view.origin(args[2]) == source
                    }) || (table.control_result_pair == Some(repr)
                        && view.projection(args[0], repr, None) == Some((source, 0))
                        && view.projection(args[2], repr, None) == Some((source, 1))))
            })
        {
            continue;
        }
        return Err("machine reference refinement has no checked use role");
    }
    Ok(())
}
