//! Quote the authoritative Core proof before the immutable CC table is frozen.

use super::ControlLayout;
use crate::BackendError;
use crate::cc::control::ControlImplementation;
use crate::cc::{
    ControlContract, ControlContractId, ControlDomain, RepresentationTable, SourceTypeIdentityId,
    TypeConstructorIdentity as C, TypeIdentity as T,
};
use psrs_core::{CheckedControlContract, ExprKind, Module, Type, TypeConstructor, TypeId};
use psrs_hir::Intrinsic;
use std::collections::HashSet;

pub(in crate::cc::layout) fn append(
    module: &Module,
    table: &mut RepresentationTable,
    layout: ControlLayout,
) -> Result<Option<psrs_hir::ModuleId>, Vec<BackendError>> {
    let mut origin = None::<usize>;
    let mut identities = Vec::<TypeId>::new();
    let mut pending = module
        .declarations
        .iter()
        .enumerate()
        .map(|(owner, declaration)| (owner, &declaration.value))
        .collect::<Vec<_>>();
    while let Some((owner, expression)) = pending.pop() {
        match &expression.kind {
            ExprKind::IntrinsicCall {
                intrinsic,
                arguments,
            } => {
                if intrinsic.descriptor().category == psrs_hir::IntrinsicCategory::Control {
                    if matches!(
                        intrinsic,
                        Intrinsic::ControlRun | Intrinsic::ControlOnce | Intrinsic::ControlCancel
                    ) {
                        origin = Some(origin.map_or(owner, |previous| previous.min(owner)));
                    }
                    let proof = psrs_core::checked_control_contract(
                        module,
                        *intrinsic,
                        expression.ty,
                        &arguments
                            .iter()
                            .map(|argument| argument.ty)
                            .collect::<Vec<_>>(),
                    )
                    .map_err(|message| {
                        vec![BackendError::invalid_ir(
                            "P8 control contract planning",
                            expression.span,
                            message,
                        )]
                    })?;
                    let quoted = quote(module, &proof).ok_or_else(|| {
                        vec![BackendError::invalid_ir(
                            "P8 control contract planning",
                            expression.span,
                            "control contract contains a dangling or cyclic type",
                        )]
                    })?;
                    let mut intern = |id: TypeId, quoted: &T| -> SourceTypeIdentityId {
                        if let Some(index) = identities
                            .iter()
                            .position(|other| psrs_core::equivalent_types(id, *other, module))
                        {
                            return SourceTypeIdentityId(index as u32);
                        }
                        identities.push(id);
                        table.plan_control_identity(quoted.clone())
                    };
                    let result_identity = intern(proof.result(), &quoted.result);
                    let argument_identities = proof
                        .arguments()
                        .iter()
                        .zip(&quoted.arguments)
                        .map(|(id, quoted)| intern(*id, quoted))
                        .collect();
                    let contract = ControlContract {
                        result_identity,
                        argument_identities,
                        implementation: implementation(layout, proof.operation()),
                        operation: proof.operation(),
                        result: quoted.result,
                        arguments: quoted.arguments,
                        source: quoted.source,
                        target: quoted.target,
                    };
                    table.plan_control_contract(contract);
                }
                pending.extend(arguments.iter().map(|value| (owner, value)));
            }
            ExprKind::Constructor { arguments, .. }
            | ExprKind::Array {
                elements: arguments,
            } => pending.extend(arguments.iter().map(|value| (owner, value))),
            ExprKind::Record { fields } => {
                pending.extend(fields.iter().map(|(_, value)| (owner, value)))
            }
            ExprKind::RecordUpdate { record, fields } => {
                pending.push((owner, record));
                pending.extend(fields.iter().map(|(_, value)| (owner, value)));
            }
            ExprKind::FieldAccess { record, .. } => pending.push((owner, record)),
            ExprKind::RepresentationCast { value, .. } => pending.push((owner, value)),
            ExprKind::Application(left, right) => {
                pending.push((owner, left));
                pending.push((owner, right));
            }
            ExprKind::Lambda { body, .. } => pending.push((owner, body)),
            ExprKind::Let { bindings, body } => {
                pending.extend(bindings.iter().map(|binding| (owner, &binding.value)));
                pending.push((owner, body));
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                pending.extend(
                    [
                        condition.as_ref(),
                        then_branch.as_ref(),
                        else_branch.as_ref(),
                    ]
                    .into_iter()
                    .map(|value| (owner, value)),
                );
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                pending.push((owner, scrutinee));
                pending.extend(branches.iter().map(|branch| (owner, &branch.value)));
            }
            _ => {}
        }
    }
    Ok(origin.map(|index| module.declarations[index].symbol.module))
}

pub(in crate::cc) fn lookup(
    module: &Module,
    table: &RepresentationTable,
    proof: &CheckedControlContract,
) -> Option<ControlContractId> {
    let quoted = quote(module, proof)?;
    table
        .control_contracts()
        .iter()
        .position(|candidate| {
            candidate.operation == proof.operation()
                && candidate.result == quoted.result
                && candidate.arguments == quoted.arguments
                && candidate.source == quoted.source
                && candidate.target == quoted.target
        })
        .map(|index| ControlContractId(index as u32))
}

struct QuotedContract {
    result: T,
    arguments: Vec<T>,
    source: Option<ControlDomain>,
    target: Option<ControlDomain>,
}

fn quote(module: &Module, proof: &CheckedControlContract) -> Option<QuotedContract> {
    let domain = |domain: psrs_core::ControlDomain| {
        Some(ControlDomain {
            request: ty(module, domain.request, &mut HashSet::new())?,
            answer: ty(module, domain.answer, &mut HashSet::new())?,
        })
    };
    Some(QuotedContract {
        result: ty(module, proof.result(), &mut HashSet::new())?,
        arguments: proof
            .arguments()
            .iter()
            .map(|id| ty(module, *id, &mut HashSet::new()))
            .collect::<Option<_>>()?,
        source: match proof.source() {
            Some(value) => Some(domain(value)?),
            None => None,
        },
        target: match proof.target() {
            Some(value) => Some(domain(value)?),
            None => None,
        },
    })
}

fn ty(module: &Module, id: TypeId, visiting: &mut HashSet<TypeId>) -> Option<T> {
    psrs_span::with_sufficient_stack(|| {
        if !visiting.insert(id) {
            return None;
        }
        let result = match module.types.get(id.0 as usize)? {
            Type::Variable(variable) => T::Variable(*variable),
            Type::Constructor(constructor) => T::Constructor(match constructor {
                TypeConstructor::Function => C::Function,
                TypeConstructor::Record => C::Record,
                TypeConstructor::Row => C::Row,
                TypeConstructor::Array => C::Array,
                TypeConstructor::Int => C::Int,
                TypeConstructor::Number => C::Number,
                TypeConstructor::Boolean => C::Boolean,
                TypeConstructor::String => C::String,
                TypeConstructor::Char => C::Char,
                TypeConstructor::Unit => C::Unit,
                TypeConstructor::Type => C::Type,
                TypeConstructor::Constraint => C::Constraint,
                TypeConstructor::Symbol => C::Symbol,
                TypeConstructor::User(id) => C::User(*id),
            }),
            Type::Application(head, argument) => T::Application(
                Box::new(ty(module, *head, visiting)?),
                Box::new(ty(module, *argument, visiting)?),
            ),
            Type::ForAll { variables, body } => T::ForAll {
                variables: variables.clone(),
                body: Box::new(ty(module, *body, visiting)?),
            },
            Type::Closure { parameters, result } => T::Closure {
                parameters: parameters
                    .iter()
                    .map(|id| ty(module, *id, visiting))
                    .collect::<Option<_>>()?,
                result: Box::new(ty(module, *result, visiting)?),
            },
            Type::RowEmpty => T::RowEmpty,
            Type::RowExtend {
                label,
                ty: field,
                tail,
            } => T::RowExtend {
                label: label.clone(),
                ty: Box::new(ty(module, *field, visiting)?),
                tail: Box::new(ty(module, *tail, visiting)?),
            },
            Type::TypeLevelString(value) => T::TypeLevelString(value.clone()),
            Type::TypeLevelInt(value) => T::TypeLevelInt(*value),
        };
        visiting.remove(&id);
        Some(result)
    })
}

fn implementation(layout: ControlLayout, operation: Intrinsic) -> ControlImplementation {
    use Intrinsic::*;
    let case = match operation {
        ControlReturn => crate::control::schema::ComputationCase::Return,
        ControlPerform => crate::control::schema::ComputationCase::Perform,
        ControlBind => crate::control::schema::ComputationCase::Bind,
        ControlHandle => crate::control::schema::ComputationCase::Handle,
        ControlResume => crate::control::schema::ComputationCase::OwnedResume,
        ControlRun => {
            return ControlImplementation::Call {
                signature: layout.run,
                role: crate::cc::ControlHelperRole::Run,
            };
        }
        ControlOnce => {
            return ControlImplementation::Call {
                signature: layout.once,
                role: crate::cc::ControlHelperRole::Once,
            };
        }
        ControlCancel => {
            return ControlImplementation::Call {
                signature: layout.cancel,
                role: crate::cc::ControlHelperRole::Cancel,
            };
        }
        _ => unreachable!("checked control operation"),
    }
    .tag();
    ControlImplementation::Frame {
        representation: layout.computation,
        case,
    }
}
