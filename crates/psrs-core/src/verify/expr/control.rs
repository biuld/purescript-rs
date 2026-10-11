//! Verify the typed source/target boundary before control payloads are erased.
use super::{Context, error, viewed};
use crate::{Expr, Module, Type, TypeConstructor, TypeId};

fn carrier_parts(
    module: &Module,
    id: TypeId,
    carrier: psrs_hir::TypeId,
    count: usize,
) -> Option<Vec<TypeId>> {
    let (_, mut id) = crate::scheme_parts(&module.types, id)?;
    let mut arguments = Vec::new();
    while let Some(Type::Application(head, argument)) = module.types.get(id.0 as usize) {
        if arguments.len() >= count {
            return None;
        }
        arguments.push(*argument);
        id = *head;
    }
    if arguments.len() != count
        || !matches!(module.types.get(id.0 as usize),
        Some(Type::Constructor(TypeConstructor::User(actual))) if *actual == carrier)
    {
        return None;
    }
    arguments.reverse();
    Some(arguments)
}

fn control_parts(module: &Module, id: TypeId) -> Option<(TypeId, TypeId)> {
    let parts = carrier_parts(module, id, psrs_hir::TypeId::CONTROL, 2)?;
    Some((parts[0], parts[1]))
}

fn pair_parts(module: &Module, id: TypeId) -> Option<(TypeId, TypeId)> {
    let fields = module.record_fields(id)?;
    if fields.len() != 2 {
        return None;
    }
    Some((
        fields.iter().find(|(label, _)| label == "_1")?.1,
        fields.iter().find(|(label, _)| label == "_2")?.1,
    ))
}

fn call(module: &Module, id: TypeId, count: usize) -> Option<(Vec<TypeId>, TypeId)> {
    let (parameters, result) = crate::call_parts(&module.types, id).ok()?;
    (parameters.len() == count).then_some((parameters, result))
}

fn request_parts(module: &Module, clause: TypeId, request: TypeId) -> Option<(TypeId, TypeId)> {
    let (variables, _) = crate::scheme_parts(&module.types, clause)?;
    let Type::Application(functor, answer) = module.types.get(request.0 as usize)? else {
        return None;
    };
    let Type::Variable(variable) = module.types.get(answer.0 as usize)? else {
        return None;
    };
    (variables.len() == 1 && variables.contains(variable)).then_some((*functor, *answer))
}

impl Context<'_> {
    pub(super) fn verify_control(
        &mut self,
        expression: &Expr,
        intrinsic: psrs_hir::Intrinsic,
        arguments: &[Expr],
    ) {
        if intrinsic == psrs_hir::Intrinsic::ControlHandle {
            return self.verify_control_handle(expression, arguments);
        }
        for argument in arguments {
            self.expr(argument, None);
        }
        let ids = std::iter::once(expression.ty)
            .chain(arguments.iter().map(|a| a.ty))
            .collect::<Vec<_>>();
        let module = viewed(self.module, self.source, &ids);
        let equal = |a, b| super::super::types::equivalent_types(a, b, module);
        let valid = (|| {
            use psrs_hir::Intrinsic::*;
            let args = arguments.iter().map(|a| a.ty).collect::<Vec<_>>();
            match intrinsic {
                ControlReturn => {
                    let (_, answer) = control_parts(module, expression.ty)?;
                    equal(args[0], answer).then_some(())
                }
                ControlPerform => {
                    let (functor, answer) = control_parts(module, expression.ty)?;
                    let Type::Application(f, a) = module.types.get(args[0].0 as usize)? else {
                        return None;
                    };
                    (equal(*f, functor) && equal(*a, answer)).then_some(())
                }
                ControlBind => {
                    let (source, answer) = control_parts(module, args[0])?;
                    let (target, _) = control_parts(module, expression.ty)?;
                    let (params, result) = call(module, args[1], 1)?;
                    (equal(source, target)
                        && equal(params[0], answer)
                        && equal(result, expression.ty))
                    .then_some(())
                }
                ControlRun => {
                    let (functor, answer) = control_parts(module, args[2])?;
                    let (params, result) = call(module, args[1], 3)?;
                    let (request, x) = request_parts(module, args[1], params[1])?;
                    let (raw, source) = call(module, params[2], 1)?;
                    let (state, next) = pair_parts(module, result)?;
                    let (out_state, out_answer) = pair_parts(module, expression.ty)?;
                    (equal(params[0], args[0])
                        && equal(request, functor)
                        && equal(raw[0], x)
                        && equal(source, args[2])
                        && equal(state, args[0])
                        && equal(next, args[2])
                        && equal(out_state, args[0])
                        && equal(out_answer, answer))
                    .then_some(())
                }
                ControlOnce => {
                    let owner =
                        carrier_parts(module, expression.ty, psrs_hir::TypeId::RESUMPTION, 3)?;
                    let (params, result) = call(module, args[0], 1)?;
                    let (functor, answer) = control_parts(module, result)?;
                    (equal(params[0], owner[1])
                        && equal(functor, owner[0])
                        && equal(answer, owner[2]))
                    .then_some(())
                }
                ControlResume => {
                    let owner = carrier_parts(module, args[0], psrs_hir::TypeId::RESUMPTION, 3)?;
                    let (functor, answer) = control_parts(module, expression.ty)?;
                    (equal(args[1], owner[1])
                        && equal(functor, owner[0])
                        && equal(answer, owner[2]))
                    .then_some(())
                }
                ControlCancel => {
                    carrier_parts(module, args[0], psrs_hir::TypeId::RESUMPTION, 3)?;
                    matches!(
                        module.types.get(expression.ty.0 as usize),
                        Some(Type::Constructor(TypeConstructor::Unit))
                    )
                    .then_some(())
                }
                _ => None,
            }
        })()
        .is_some();
        if !valid {
            self.errors.push(error(
                self.owner,
                expression.span,
                "control primitive does not preserve its checked carrier and callable contracts",
            ));
        }
    }

    pub(super) fn verify_control_handle(&mut self, expression: &Expr, arguments: &[Expr]) {
        for argument in arguments {
            self.expr(argument, None);
        }
        let module = viewed(
            self.module,
            self.source,
            &[
                expression.ty,
                arguments[0].ty,
                arguments[1].ty,
                arguments[2].ty,
            ],
        );
        let valid = (|| {
            let (source_request, source_answer) = control_parts(module, arguments[2].ty)?;
            control_parts(module, expression.ty)?;
            let (ret_parameters, ret_result) =
                crate::call_parts(&module.types, arguments[0].ty).ok()?;
            if ret_parameters.len() != 1 {
                return None;
            }
            let (variables, body) = crate::scheme_parts(&module.types, arguments[1].ty)?;
            let (parameters, result) = crate::call_parts(&module.types, body).ok()?;
            if variables.len() != 1 || parameters.len() != 2 {
                return None;
            }
            let Type::Application(request, answer) = module.types.get(parameters[0].0 as usize)?
            else {
                return None;
            };
            let Type::Variable(variable) = module.types.get(answer.0 as usize)? else {
                return None;
            };
            if !variables.contains(variable) {
                return None;
            }
            let (raw_parameters, raw_result) =
                crate::call_parts(&module.types, parameters[1]).ok()?;
            if raw_parameters.len() != 1 {
                return None;
            }
            // Use the checking owner's single rigid relation for all repeated
            // positions. Quantified answer variables must remain the same.
            let equal = |a, b| super::super::types::equivalent_types(a, b, module);
            (equal(ret_parameters[0], source_answer)
                && equal(ret_result, expression.ty)
                && equal(*request, source_request)
                && equal(raw_parameters[0], *answer)
                && equal(raw_result, arguments[2].ty)
                && equal(result, expression.ty))
            .then_some(())
        })()
        .is_some();
        if !valid {
            self.errors.push(error(
                self.owner,
                expression.span,
                "control handle clauses do not preserve their checked source and target contracts",
            ));
        }
    }
}
