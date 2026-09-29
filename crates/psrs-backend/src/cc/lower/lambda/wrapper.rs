use super::super::LoweringContext;
use super::super::{Assignment, AssignmentKind, Function, ValueDecl, ValueId, ValueShape};
use super::{closure_value_type, closure_value_type_for, is_erased_reference};
use crate::cc::{RefShape, Reference};
use psrs_core::Declaration;

pub(in crate::cc::lower) fn make_wrapper(
    source: &Function,
    declaration: &Declaration,
    context: &LoweringContext<'_>,
) -> Function {
    let symbol = context.function_wrappers[&declaration.symbol];
    if let Some(wrapper) = eta_expanded_wrapper(source, declaration, context, symbol) {
        return wrapper;
    }
    let closure = ValueId(0);
    let mut values = vec![ValueDecl {
        id: closure,
        ty: closure_value_type(),
    }];
    let mut parameters = vec![closure];
    let mut arguments = Vec::with_capacity(source.parameters.len());
    for (index, parameter) in source.parameters.iter().enumerate() {
        let id = ValueId(index as u32 + 1);
        let ty = source
            .values
            .iter()
            .find(|value| value.id == *parameter)
            .map_or(ValueShape::Integer, |value| value.ty);
        values.push(ValueDecl { id, ty });
        parameters.push(id);
        arguments.push(id);
    }
    let result = ValueId(parameters.len() as u32);
    values.push(ValueDecl {
        id: result,
        ty: source.result_type,
    });
    Function {
        symbol,
        name: format!("{}_closure_wrapper", source.name),
        parameters,
        values,
        assignments: vec![Assignment {
            destination: result,
            kind: AssignmentKind::DirectCall {
                function: source.symbol,
                arguments,
            },
            span: declaration.span,
        }],
        result,
        result_type: source.result_type,
        span: declaration.span,
    }
}

/// Builds a wrapper that exposes the declaration's full, flattened function
/// type. A declaration such as `f x = g x` binds fewer parameters than its
/// type has arrows, so its own function returns a closure; the wrapper applies
/// the remaining parameters with an indirect call. Without this the closure's
/// arity would disagree with the type used at every value use site.
fn eta_expanded_wrapper(
    source: &Function,
    declaration: &Declaration,
    context: &LoweringContext<'_>,
    symbol: psrs_hir::SymbolId,
) -> Option<Function> {
    if source.parameters.is_empty() {
        return None;
    }
    let flattened_id = *context.function_types.get(&declaration.ty)?;
    let flattened = context.representations.signature(flattened_id)?;
    let peeled = source.parameters.len();
    if flattened.parameters.len() <= peeled {
        return None;
    }
    let body_type = peel_function_type(context.module, declaration.ty, peeled)?;
    let body_signature = *context.function_types.get(&body_type)?;
    let erased = is_erased_reference(source.result_type);
    if source.result_type != closure_value_type_for(body_signature) && !erased {
        return None;
    }
    let mut values = vec![ValueDecl {
        id: ValueId(0),
        ty: closure_value_type(),
    }];
    let mut parameters = vec![ValueId(0)];
    let mut flattened_arguments = Vec::with_capacity(flattened.parameters.len());
    for (index, parameter_type) in flattened.parameters.iter().enumerate() {
        let id = ValueId(index as u32 + 1);
        values.push(ValueDecl {
            id,
            ty: *parameter_type,
        });
        parameters.push(id);
        flattened_arguments.push(id);
    }
    let intermediate = ValueId(flattened.parameters.len() as u32 + 1);
    let result = ValueId(flattened.parameters.len() as u32 + 2);
    values.push(ValueDecl {
        id: intermediate,
        ty: source.result_type,
    });
    values.push(ValueDecl {
        id: result,
        ty: flattened.result,
    });
    let first_parameters = flattened_arguments[..peeled].to_vec();
    let remaining_parameters = flattened_arguments[peeled..].to_vec();
    let mut assignments = vec![Assignment {
        destination: intermediate,
        kind: AssignmentKind::DirectCall {
            function: source.symbol,
            arguments: first_parameters,
        },
        span: declaration.span,
    }];
    // A generic body result is erased; recover the closure shape before
    // applying the remaining effect parameters.
    let callable = if erased {
        let closure = ValueId(flattened.parameters.len() as u32 + 3);
        values.push(ValueDecl {
            id: closure,
            ty: closure_value_type_for(body_signature),
        });
        assignments.push(Assignment {
            destination: closure,
            kind: AssignmentKind::RepresentationCast {
                destination: closure,
                value: intermediate,
                reference: Reference {
                    nullable: false,
                    heap: RefShape::Closure(body_signature),
                },
            },
            span: declaration.span,
        });
        closure
    } else {
        intermediate
    };
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::IndirectCall {
            function: callable,
            signature: body_signature,
            arguments: remaining_parameters,
        },
        span: declaration.span,
    });
    Some(Function {
        symbol,
        name: format!("{}_closure_wrapper", source.name),
        parameters,
        values,
        assignments,
        result,
        result_type: flattened.result,
        span: declaration.span,
    })
}

fn peel_function_type(
    module: &psrs_core::Module,
    mut ty: psrs_core::TypeId,
    count: usize,
) -> Option<psrs_core::TypeId> {
    for _ in 0..count {
        let (_, result) = psrs_core::arrow_parts(&module.types, ty)?;
        ty = result;
    }
    Some(ty)
}
