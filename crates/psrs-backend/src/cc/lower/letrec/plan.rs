//! Plan typed memo slots before immutable CC lowering begins.
use super::{groups::binding_groups, groups::references_itself};
use crate::BackendError;
use crate::cc::layout::{TypeLayout, scalar_type};
use crate::cc::{Representation, Signature, VariantCase};
use psrs_core::{ExprKind, Module};
use psrs_hir::TypeId;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::cc) struct RecursiveLayout {
    pub state: crate::cc::ReprId,
    pub cell: crate::cc::ReprId,
    pub getter: crate::cc::SignatureId,
}

pub(in crate::cc) fn plan_recursive_values(
    module: &Module,
    layout: &mut TypeLayout,
    enums: &HashSet<TypeId>,
    aggregates: &HashSet<TypeId>,
    newtypes: &HashSet<TypeId>,
) -> Result<(), Vec<BackendError>> {
    let mut pending = module
        .declarations
        .iter()
        .map(|d| &d.value)
        .collect::<Vec<_>>();
    while let Some(expr) = pending.pop() {
        match &expr.kind {
            ExprKind::Let { bindings, body } => {
                for group in binding_groups(bindings) {
                    if (group.len() > 1 || references_itself(&bindings[group[0]]))
                        && !super::functions_only(module, &layout.function_types, bindings, &group)
                    {
                        for index in group {
                            let b = &bindings[index].binder;
                            let shape = scalar_type(
                                module,
                                b.ty,
                                b.span,
                                enums,
                                aggregates,
                                newtypes,
                                &layout.nominal_types,
                                &layout.array_types,
                                &layout.record_types,
                                &layout.function_types,
                            )?;
                            if layout.representations.recursion.contains_key(&shape) {
                                continue;
                            }
                            let table = &mut layout.representations;
                            let state = table.reserve();
                            table.set(
                                state,
                                Representation::Variant {
                                    cases: vec![
                                        VariantCase {
                                            tag: 0,
                                            fields: vec![],
                                        },
                                        VariantCase {
                                            tag: 1,
                                            fields: vec![],
                                        },
                                        VariantCase {
                                            tag: 2,
                                            fields: vec![shape],
                                        },
                                    ],
                                },
                            );
                            let cell = table.reserve();
                            table.set(
                                cell,
                                Representation::Array {
                                    element: super::repr(state),
                                },
                            );
                            let getter = table.add_signature(Signature {
                                parameters: vec![],
                                result: shape,
                            });
                            table.recursion.insert(
                                shape,
                                RecursiveLayout {
                                    state,
                                    cell,
                                    getter,
                                },
                            );
                        }
                    }
                }
                pending.push(body);
                pending.extend(bindings.iter().map(|b| &b.value));
            }
            ExprKind::Lambda { body, .. }
            | ExprKind::RepresentationCast { value: body, .. }
            | ExprKind::FieldAccess { record: body, .. } => pending.push(body),
            ExprKind::Application(a, b) => pending.extend([a.as_ref(), b.as_ref()]),
            ExprKind::Array { elements }
            | ExprKind::Constructor {
                arguments: elements,
                ..
            }
            | ExprKind::IntrinsicCall {
                arguments: elements,
                ..
            } => pending.extend(elements),
            ExprKind::Record { fields } => pending.extend(fields.iter().map(|(_, e)| e)),
            ExprKind::RecordUpdate { record, fields } => {
                pending.push(record);
                pending.extend(fields.iter().map(|(_, e)| e));
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                pending.push(scrutinee);
                pending.extend(branches.iter().map(|b| &b.value));
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => pending.extend([
                condition.as_ref(),
                then_branch.as_ref(),
                else_branch.as_ref(),
            ]),
            ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Integer(_)
            | ExprKind::Number(_)
            | ExprKind::Boolean(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Unit
            | ExprKind::Trap => {}
        }
    }
    Ok(())
}
