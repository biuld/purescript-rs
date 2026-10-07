use super::{Locals, SchemeType};
use crate::{Module, Type, TypeConstructor, TypeId, VerifyError};
use psrs_hir::{Intrinsic, LocalId, ModuleId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

pub(super) fn verify_type(
    id: TypeId,
    module: &Module,
    owner: ModuleId,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    if id.0 as usize >= module.types.len() {
        errors.push(error(
            owner,
            span,
            "type reference is outside the Core type table",
        ));
    }
}

pub(super) fn primitive_types(intrinsic: Intrinsic, module: &Module) -> (TypeId, TypeId) {
    use TypeConstructor::{Boolean, Char, Int, Number};
    let (operand_type, result_type) = match intrinsic {
        Intrinsic::IntAdd
        | Intrinsic::IntSub
        | Intrinsic::IntMul
        | Intrinsic::IntQuot
        | Intrinsic::IntRem
        | Intrinsic::IntAnd
        | Intrinsic::IntOr
        | Intrinsic::IntXor
        | Intrinsic::IntShl
        | Intrinsic::IntShr
        | Intrinsic::IntZshr => (Int, Int),
        Intrinsic::IntEq
        | Intrinsic::IntNe
        | Intrinsic::IntLt
        | Intrinsic::IntLe
        | Intrinsic::IntGt
        | Intrinsic::IntGe => (Int, Boolean),
        Intrinsic::CharEq
        | Intrinsic::CharNe
        | Intrinsic::CharLt
        | Intrinsic::CharLe
        | Intrinsic::CharGt
        | Intrinsic::CharGe => (Char, Boolean),
        Intrinsic::NumberAdd
        | Intrinsic::NumberSub
        | Intrinsic::NumberMul
        | Intrinsic::NumberDiv => (Number, Number),
        Intrinsic::NumberEq
        | Intrinsic::NumberNe
        | Intrinsic::NumberLt
        | Intrinsic::NumberLe
        | Intrinsic::NumberGt
        | Intrinsic::NumberGe => (Number, Boolean),
        Intrinsic::BooleanAnd
        | Intrinsic::BooleanOr
        | Intrinsic::BooleanEq
        | Intrinsic::BooleanNe => (Boolean, Boolean),
        _ => unreachable!(
            "primitive_types is only called for a binary scalar intrinsic, got {intrinsic:?}"
        ),
    };
    (
        primitive_type_id(module, operand_type),
        primitive_type_id(module, result_type),
    )
}

pub(super) fn unary_primitive_types(intrinsic: Intrinsic, module: &Module) -> (TypeId, TypeId) {
    use TypeConstructor::{Boolean, Char, Int, Number, String};
    let (operand, result) = match intrinsic {
        Intrinsic::IntNeg | Intrinsic::IntComplement => (Int, Int),
        Intrinsic::NumberNeg
        | Intrinsic::NumberTrunc
        | Intrinsic::NumberFloor
        | Intrinsic::NumberCeil
        | Intrinsic::NumberAbs
        | Intrinsic::NumberSqrt => (Number, Number),
        Intrinsic::BooleanNot => (Boolean, Boolean),
        Intrinsic::IntToNumber => (Int, Number),
        Intrinsic::NumberToInt => (Number, Int),
        Intrinsic::NumberToString => (Number, String),
        Intrinsic::NumberFromDecimal => (String, Number),
        Intrinsic::BooleanToInt => (Boolean, Int),
        Intrinsic::IntToBoolean => (Int, Boolean),
        Intrinsic::CharToInt => (Char, Int),
        Intrinsic::IntToChar => (Int, Char),
        _ => unreachable!(
            "unary_primitive_types is only called for a unary scalar intrinsic, got {intrinsic:?}"
        ),
    };
    (
        primitive_type_id(module, operand),
        primitive_type_id(module, result),
    )
}

/// The type id of a primitive scalar constructor, or an out-of-range id when
/// the module has no such scalar. The runtime mapping from a primitive
/// constructor lives at the backend layout boundary.
pub(super) fn primitive_type_id(module: &Module, constructor: TypeConstructor) -> TypeId {
    type_id_for(module, &Type::Constructor(constructor))
}

pub(super) fn type_id_for(module: &Module, shape: &Type) -> TypeId {
    module
        .types
        .iter()
        .position(|candidate| candidate == shape)
        .map_or(TypeId(u32::MAX), |index| TypeId(index as u32))
}

pub(super) fn array_element(id: TypeId, module: &Module) -> Option<TypeId> {
    let Type::Application(function, argument) = module.types.get(id.0 as usize)? else {
        return None;
    };
    matches!(
        module.types.get(function.0 as usize),
        Some(Type::Constructor(crate::TypeConstructor::Array))
    )
    .then_some(*argument)
}

pub(super) fn record_field(id: TypeId, label: &str, module: &Module) -> Option<TypeId> {
    module.record_field(id, label)
}

mod matching;
pub(crate) use matching::equivalent_types;
pub(crate) use matching::instantiation;
pub(super) use matching::{
    application_matches, compatible, constructor_fields_match, scheme_instance,
};

fn types_compatible(
    left: TypeId,
    right: TypeId,
    module: &Module,
    seen: &mut HashSet<(TypeId, TypeId)>,
    alpha: &mut HashMap<psrs_hir::TypeVariableId, psrs_hir::TypeVariableId>,
) -> bool {
    if left == right || !seen.insert((left, right)) {
        return true;
    }
    let (Some(left), Some(right)) = (
        module.types.get(left.0 as usize),
        module.types.get(right.0 as usize),
    ) else {
        return false;
    };
    match (left, right) {
        (Type::Variable(left), Type::Variable(right)) => {
            alpha.get(left).copied().unwrap_or(*left) == *right
                && !alpha
                    .iter()
                    .any(|(bound, target)| bound != left && *target == *right)
        }
        (
            Type::ForAll {
                variables: left_variables,
                body: left_body,
            },
            Type::ForAll {
                variables: right_variables,
                body: right_body,
            },
        ) if left_variables.len() == right_variables.len() => {
            let mut added = Vec::new();
            for (left, right) in left_variables.iter().zip(right_variables) {
                if alpha.insert(*left, *right).is_some() {
                    for variable in added {
                        alpha.remove(&variable);
                    }
                    return false;
                }
                added.push(*left);
            }
            let equal = types_compatible(*left_body, *right_body, module, seen, alpha);
            for variable in added {
                alpha.remove(&variable);
            }
            equal
        }
        (Type::Constructor(a), Type::Constructor(b)) => a == b,
        (Type::Application(a1, a2), Type::Application(b1, b2))
            if is_record_head(module, *a1) && is_record_head(module, *b1) =>
        {
            record_rows_compatible(module, *a2, *b2, seen, alpha)
        }
        (Type::Application(a1, a2), Type::Application(b1, b2)) => {
            types_compatible(*a1, *b1, module, seen, alpha)
                && types_compatible(*a2, *b2, module, seen, alpha)
        }
        (
            Type::Closure {
                parameters: left_parameters,
                result: left_result,
            },
            Type::Closure {
                parameters: right_parameters,
                result: right_result,
            },
        ) if left_parameters.len() == right_parameters.len() => {
            left_parameters
                .iter()
                .zip(right_parameters)
                .all(|(left, right)| types_compatible(*left, *right, module, seen, alpha))
                && types_compatible(*left_result, *right_result, module, seen, alpha)
        }
        (Type::RowEmpty, Type::RowEmpty) => true,
        (
            Type::RowExtend {
                label: left_label,
                ty: left_ty,
                tail: left_tail,
            },
            Type::RowExtend {
                label: right_label,
                ty: right_ty,
                tail: right_tail,
            },
        ) => {
            left_label == right_label
                && types_compatible(*left_ty, *right_ty, module, seen, alpha)
                && types_compatible(*left_tail, *right_tail, module, seen, alpha)
        }
        _ => false,
    }
}

fn is_record_head(module: &Module, id: TypeId) -> bool {
    matches!(
        module.types.get(id.0 as usize),
        Some(Type::Constructor(TypeConstructor::Record))
    )
}

/// Compares two record rows. A closed left row requires the same labels as the
/// right; an open left row requires every one of its labels to be present on the
/// right. A closed left row is never compatible with an open right row.
fn record_rows_compatible(
    module: &Module,
    left_row: TypeId,
    right_row: TypeId,
    seen: &mut HashSet<(TypeId, TypeId)>,
    alpha: &mut HashMap<psrs_hir::TypeVariableId, psrs_hir::TypeVariableId>,
) -> bool {
    let (Some((left_fields, left_tail)), Some((right_fields, right_tail))) =
        (module.row_fields(left_row), module.row_fields(right_row))
    else {
        return false;
    };
    let mut present = |fields: &[(String, TypeId)]| {
        left_fields.iter().all(|(label, ty)| {
            fields
                .iter()
                .find(|(other, _)| other == label)
                .is_some_and(|(_, other)| types_compatible(*ty, *other, module, seen, alpha))
        })
    };
    match (left_tail, right_tail) {
        (None, None) => left_fields.len() == right_fields.len() && present(&right_fields),
        (Some(_), _) => present(&right_fields),
        (None, Some(_)) => false,
    }
}

pub(super) fn restore_local(locals: &mut Locals, id: LocalId, previous: Option<SchemeType>) {
    if let Some(previous) = previous {
        locals.insert(id, previous);
    } else {
        locals.remove(&id);
    }
}

pub(super) fn error(module: ModuleId, span: TextRange, message: &'static str) -> VerifyError {
    VerifyError {
        module,
        span,
        message,
    }
}
