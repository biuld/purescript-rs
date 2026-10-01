use super::super::super::layout::{
    callable_application, function_arrow_parameters, unquantified_type,
};
use super::super::super::{
    Assignment, AssignmentKind, RefShape, Reference, SignatureId, ValueConversion, ValueId,
    ValueShape,
};
use super::super::FunctionLowerer;
use psrs_core::{Expr, ExprKind, Module as CoreModule};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;

pub(super) fn collect_application<'a>(
    module: &CoreModule,
    local_types: &HashMap<psrs_hir::LocalId, psrs_core::TypeId>,
    expression: &'a Expr,
) -> (&'a Expr, Vec<&'a Expr>) {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let ExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
        // A callable constructor application (an `Effect a` value) is itself
        // callable, so applying it supplies its hidden context parameter. Stop
        // flattening there so the remaining arguments belong to the closure it
        // returns rather than to the outer function.
        if callable_application(module, head.ty).is_some() {
            break;
        }
        // A polymorphic result is a separate closure value. Stop here so an
        // expression like `make 0 42`, where `make 0` returns `forall a. a ->
        // a`, lowers as two calls with the second using that closure's own
        // signature.
        if application_returns_forall(module, local_types, head) {
            break;
        }
    }
    arguments.reverse();
    (head, arguments)
}

/// A call's result type can be instantiated at the use site, so its expression
/// type may no longer contain the quantifier that marks the declaration's
/// closure boundary. Recover that boundary from the raw callee scheme when an
/// application spine reaches the declaration's full ordinary arity.
fn application_returns_forall(
    module: &CoreModule,
    local_types: &HashMap<psrs_hir::LocalId, psrs_core::TypeId>,
    expression: &Expr,
) -> bool {
    if psrs_core::forall_parts(&module.types, expression.ty).is_some() {
        return true;
    }
    let mut root = expression;
    let mut applied = 0usize;
    while let ExprKind::Application(function, _) = &root.kind {
        root = function;
        applied += 1;
    }
    let source_type = match &root.kind {
        ExprKind::Global(symbol) => module
            .declarations
            .iter()
            .find(|declaration| declaration.symbol == *symbol)
            .map(|declaration| declaration.ty),
        ExprKind::Local(local) => local_types.get(local).copied().or(Some(root.ty)),
        _ => Some(root.ty),
    };
    let Some(source_type) = source_type else {
        return false;
    };
    let mut remaining = applied;
    let mut current = source_type;
    for _ in 0..=module.types.len() {
        let (parameters, result) = function_arrow_parameters(module, current);
        if remaining == parameters.len() {
            return psrs_core::forall_parts(&module.types, result).is_some();
        }
        if remaining < parameters.len() || parameters.is_empty() {
            return false;
        }
        remaining -= parameters.len();
        let Some((_, body)) = psrs_core::forall_parts(&module.types, result) else {
            return false;
        };
        current = body;
    }
    false
}

/// Whether a value is a callable closure: an ordinary function arrow or the
/// closure representation of a registered callable constructor (for example an
/// `Effect a`). Both participate in the erased callable protocol, so a callable
/// constructor's value is handled by the same adapters as a function.
pub(in crate::cc::lower) fn is_function_type(
    module: &CoreModule,
    type_id: psrs_core::TypeId,
) -> bool {
    let type_id = unquantified_type(module, type_id);
    psrs_core::arrow_parts(&module.types, type_id).is_some()
        || callable_application(module, type_id).is_some()
}

pub(super) fn closure_value_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

pub(super) fn closure_value_type_for(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

pub(super) fn declaration_parameter_types(
    module: &CoreModule,
    symbol: SymbolId,
) -> Vec<psrs_core::TypeId> {
    let Some(declaration) = module
        .declarations
        .iter()
        .find(|item| item.symbol == symbol)
    else {
        return Vec::new();
    };
    function_arrow_parameters(module, declaration.ty).0
}

/// The value type a declaration produces after its ordinary arguments: its
/// declared type with every arrow peeled, stopping before a callable
/// constructor's hidden parameters. For `discard :: Effect a -> (a -> Effect
/// b) -> Effect b` this is `Effect b`, not the value `b` inside the effect.
pub(super) fn declaration_result_type(
    module: &CoreModule,
    symbol: SymbolId,
) -> Option<psrs_core::TypeId> {
    let declaration = module
        .declarations
        .iter()
        .find(|declaration| declaration.symbol == symbol)?;
    Some(function_arrow_parameters(module, declaration.ty).1)
}

/// The result type of a (possibly curried) function type: the value produced
/// after the last ordinary argument, stopping before an effect's returned
/// function's own arguments. A non-function type is its own result.
pub(super) fn function_result_type(
    module: &CoreModule,
    type_id: psrs_core::TypeId,
) -> psrs_core::TypeId {
    function_arrow_parameters(module, type_id).1
}

pub(super) fn callable_parameter_types(
    module: &CoreModule,
    symbol: SymbolId,
    callable_type: psrs_core::TypeId,
) -> Vec<psrs_core::TypeId> {
    if module
        .declarations
        .iter()
        .any(|declaration| declaration.symbol == symbol)
    {
        return declaration_parameter_types(module, symbol);
    }
    function_parameter_types(module, callable_type)
}

pub(super) fn callable_result_type(
    module: &CoreModule,
    symbol: SymbolId,
    callable_type: psrs_core::TypeId,
) -> Option<psrs_core::TypeId> {
    declaration_result_type(module, symbol).or_else(|| {
        let (_, result) = function_arrow_parameters(module, callable_type);
        module.types.get(result.0 as usize).map(|_| result)
    })
}

fn function_parameter_types(
    module: &CoreModule,
    type_id: psrs_core::TypeId,
) -> Vec<psrs_core::TypeId> {
    function_arrow_parameters(module, type_id).0
}

/// The source parameter and result types of a function-typed value, used to
/// adapt concrete arguments and results across an erased method call.
pub(super) fn function_value_types(
    module: &CoreModule,
    type_id: psrs_core::TypeId,
) -> (Vec<psrs_core::TypeId>, psrs_core::TypeId) {
    function_arrow_parameters(module, type_id)
}

pub(super) fn conversion_reconstructs_aggregate(conversion: &ValueConversion) -> bool {
    match conversion {
        ValueConversion::ArrayMap { .. } | ValueConversion::ProductMap { .. } => true,
        ValueConversion::Sequence(steps) => steps.iter().any(conversion_reconstructs_aggregate),
        ValueConversion::Identity
        | ValueConversion::BoxScalar { .. }
        | ValueConversion::UnboxScalar { .. }
        | ValueConversion::EraseReference
        | ValueConversion::RecoverReference { .. }
        | ValueConversion::FunctionAdapter { .. } => false,
    }
}

pub(in crate::cc::lower) fn persist_reference(
    lowerer: &mut FunctionLowerer<'_>,
    value: ValueId,
    shape: ValueShape,
    span: TextRange,
    assignments: &mut Vec<Assignment>,
) -> (ValueId, Option<ValueShape>) {
    let ValueShape::Reference(reference) = shape else {
        return (value, None);
    };
    if reference.nullable {
        return (value, None);
    }
    let nullable = nullable_reference_shape(shape);
    let result = lowerer.fresh(nullable);
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::RepresentationCast {
            destination: result,
            value,
            reference: reference_of(nullable),
        },
        span,
    });
    (result, Some(shape))
}

pub(in crate::cc::lower) fn restore_reference(
    lowerer: &mut FunctionLowerer<'_>,
    value: ValueId,
    shape: ValueShape,
    span: TextRange,
    assignments: &mut Vec<Assignment>,
) -> ValueId {
    let result = lowerer.fresh(shape);
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::RepresentationCast {
            destination: result,
            value,
            reference: reference_of(shape),
        },
        span,
    });
    result
}

pub(super) fn nullable_reference_shape(shape: ValueShape) -> ValueShape {
    let ValueShape::Reference(mut reference) = shape else {
        unreachable!("only references can be persisted across aggregate loops")
    };
    reference.nullable = true;
    ValueShape::Reference(reference)
}

fn reference_of(shape: ValueShape) -> Reference {
    let ValueShape::Reference(reference) = shape else {
        unreachable!("reference cast target must be a reference")
    };
    reference
}
