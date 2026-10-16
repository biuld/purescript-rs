//! Eligibility at declaration-owned callable storage boundaries.
//!
//! Specializing a generic body which opens or produces such a field changes
//! its use signature while the field's declared convention stays generic.
//! Until lowering can eliminate that round trip, retain the generic entry:
//! added result adapters can turn a constant-stack invocation into recursion
//! with post-call work. This is independent of State and library names.

use crate::{Declaration, Module, Type, TypeConstructor, TypeId, arrow_parts};
use std::collections::HashSet;

pub(super) fn crosses_callable_field(module: &Module, declaration: &Declaration) -> bool {
    // Examine each ordinary argument and the final result, rather than
    // classifying the declaration's own outer arrows as stored callables.
    let mut result = declaration.ty;
    while let Some((parameter, next)) = arrow_parts(&module.types, result) {
        if contains_callable_field(module, parameter, &mut HashSet::new()) {
            return true;
        }
        result = next;
    }
    contains_callable_field(module, result, &mut HashSet::new())
}

fn contains_callable_field(module: &Module, ty: TypeId, seen: &mut HashSet<TypeId>) -> bool {
    if !seen.insert(ty) {
        return false;
    }
    if let Some((head, arguments)) = module.applied_constructor(ty) {
        if let TypeConstructor::User(owner) = head {
            for constructor in &module.constructors {
                if constructor.type_id == owner
                    && constructor.field_types.iter().any(|field| {
                        arrow_parts(&module.types, *field).is_some()
                            || contains_callable_field(module, *field, seen)
                    })
                {
                    return true;
                }
            }
        }
        if arguments
            .iter()
            .any(|argument| contains_callable_field(module, *argument, seen))
        {
            return true;
        }
    }
    match module.types.get(ty.0 as usize) {
        Some(Type::Application(function, argument)) => {
            contains_callable_field(module, *function, seen)
                || contains_callable_field(module, *argument, seen)
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            contains_callable_field(module, *ty, seen)
                || contains_callable_field(module, *tail, seen)
        }
        Some(Type::ForAll { body, .. }) => contains_callable_field(module, *body, seen),
        Some(Type::Closure { parameters, result }) => {
            parameters
                .iter()
                .any(|parameter| contains_callable_field(module, *parameter, seen))
                || contains_callable_field(module, *result, seen)
        }
        _ => false,
    }
}
