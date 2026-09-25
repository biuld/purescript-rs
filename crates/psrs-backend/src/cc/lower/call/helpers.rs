use super::super::super::layout::depends_on_type_variable;
use super::super::super::{
    Assignment, AssignmentKind, RefShape, Reference, SignatureId, ValueConversion, ValueId,
    ValueShape,
};
use super::super::FunctionLowerer;
use psrs_core::{Expr, ExprKind, Module as CoreModule, Type};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

pub(super) fn collect_application(expression: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let ExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}

pub(in crate::cc::lower) fn is_erased_value_type(value_type: ValueShape) -> bool {
    matches!(
        value_type,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        })
    )
}

pub(super) fn is_function_type(module: &CoreModule, type_id: psrs_core::TypeId) -> bool {
    matches!(
        module.types.get(type_id.0 as usize),
        Some(Type::Function { .. })
    )
}

pub(in crate::cc::lower) fn is_generic_function_type(
    module: &CoreModule,
    type_id: psrs_core::TypeId,
) -> bool {
    is_function_type(module, type_id) && depends_on_type_variable(module, type_id)
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
    let mut parameters = Vec::new();
    let mut type_id = declaration.ty;
    while let Some(Type::Function { parameter, result }) = module.types.get(type_id.0 as usize) {
        parameters.push(*parameter);
        type_id = *result;
    }
    parameters
}

pub(super) fn declaration_result_type(
    module: &CoreModule,
    symbol: SymbolId,
) -> Option<psrs_core::TypeId> {
    let declaration = module
        .declarations
        .iter()
        .find(|declaration| declaration.symbol == symbol)?;
    let mut type_id = declaration.ty;
    while let Some(Type::Function { result, .. }) = module.types.get(type_id.0 as usize) {
        type_id = *result;
    }
    Some(type_id)
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
        let mut type_id = callable_type;
        while let Some(Type::Function { result, .. }) = module.types.get(type_id.0 as usize) {
            type_id = *result;
        }
        module.types.get(type_id.0 as usize).map(|_| type_id)
    })
}

fn function_parameter_types(
    module: &CoreModule,
    mut type_id: psrs_core::TypeId,
) -> Vec<psrs_core::TypeId> {
    let mut parameters = Vec::new();
    while let Some(Type::Function { parameter, result }) = module.types.get(type_id.0 as usize) {
        parameters.push(*parameter);
        type_id = *result;
    }
    parameters
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
