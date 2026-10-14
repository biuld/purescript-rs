//! Refinements need static subtyping, a selected case, or a checked callback role.
use super::execution::View;
use crate::control::execution::Operands;
use crate::mir::branch_facts::Exit;
use crate::mir::layout::PlannedLayout;
use crate::mir::{Function, Instruction as I, Terminator as T};
use crate::types::{HeapType, RefType, ValueId, ValueType};

pub(super) fn checked(
    view: &View<'_>,
    function: &Function,
    layout: &PlannedLayout,
    path: &Exit,
    value: ValueId,
    target: RefType,
) -> bool {
    let definitions = view.definitions;
    let source = view.origin(value);
    let defined = layout.types().iter().flat_map(|g| &g.0).collect::<Vec<_>>();
    if function
        .values
        .iter()
        .find(|v| v.id == source)
        .is_some_and(|v| match v.ty {
            ValueType::Ref(actual) => {
                (target.nullable || !actual.nullable)
                    && crate::mir::verify::heap_subtype(actual.heap, target.heap, &defined)
            }
            _ => false,
        })
    {
        return true;
    }
    let applied = path.points.iter().any(|(id,index)| function.blocks.iter().find(|b| b.id == *id).and_then(|b| b.instructions.get(*index)).is_some_and(|i| matches!(i, I::ClosureCall { function: callee, .. } if view.origin(*callee) == source)));
    if target.heap == HeapType::Eq && applied {
        return true;
    }
    let HeapType::Index(target_index) = target.heap else {
        return false;
    };
    let table = layout.representation_table();
    for (index, repr) in table.representations.iter().enumerate() {
        let repr_id = crate::cc::ReprId(index as u32);
        if let crate::cc::Representation::Variant { cases } = repr
            && cases.len() == 1
            && layout.variant_index(repr_id, cases[0].tag).ok() == Some(target_index)
            && let Some(base) = layout.repr_index(repr_id).ok() && function.values.iter().find(|v| v.id == source).is_some_and(|v| matches!(v.ty, ValueType::Ref(actual) if !actual.nullable && crate::mir::verify::heap_subtype(actual.heap, HeapType::Index(base), &defined))) { return true; }
    }
    // A physical case cast is licensed by this path's immutable selected tag.
    for ((base, selected), tag) in &path.facts {
        if view.origin(*selected) != source {
            continue;
        }
        for (index, repr) in table.representations.iter().enumerate() {
            let repr_id = crate::cc::ReprId(index as u32);
            if matches!(repr, crate::cc::Representation::Variant { .. })
                && layout.repr_index(repr_id).ok() == Some(*base)
                && layout.variant_index(repr_id, *tag as u32).ok() == Some(target_index)
            {
                return true;
            }
        }
    }
    // Ordinary callback ABI recovery is justified only for an actual applied
    // callee. The owning execution relation checks the complete application and
    // its checked source role, including its returned Control computation.
    if layout
        .closure_layout()
        .ok()
        .is_some_and(|(closure, _)| closure == target_index)
    {
        return applied;
    }
    let Some(schema) = table.control_checkpoint_schema() else {
        return false;
    };
    let Some(block) = function.blocks.iter().find(|b| b.id == path.block) else {
        return false;
    };
    let arguments = match &block.terminator {
        Some(T::Jump { arguments, .. } | T::ReturnCall { arguments, .. }) => {
            Some(arguments.as_slice())
        }
        Some(T::Return { value, .. }) => match definitions.get(&view.origin(*value)).copied() {
            Some(I::Call {
                function: callee,
                arguments,
                ..
            }) if *callee == function.symbol => Some(arguments.as_slice()),
            _ => None,
        },
        _ => None,
    };
    if let Some(pair) = table.control_result_pair
        && layout.repr_index(pair).ok() == Some(target_index)
    {
        return arguments.is_some_and(|args| {
            args.len() == 6
                && view.projection(args[0], pair, None) == Some((source, 0))
                && view.projection(args[2], pair, None) == Some((source, 1))
        });
    }
    layout.repr_index(schema.computation).ok() == Some(target_index)
        && arguments.is_some_and(|args| args.len() == 6 && view.origin(args[2]) == source)
}
