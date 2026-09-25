use super::Locals;
use crate::{Module, Primitive, Type, TypeConstructor, TypeId, UnaryPrimitive, VerifyError};
use psrs_hir::TypeId as HirTypeId;
use psrs_hir::{LocalId, ModuleId};
use psrs_span::TextRange;
use std::collections::HashSet;

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

pub(super) fn primitive_types(op: Primitive, module: &Module) -> (TypeId, TypeId) {
    let (operand_type, result_type) = match op {
        Primitive::IntAdd
        | Primitive::IntSub
        | Primitive::IntMul
        | Primitive::IntQuot
        | Primitive::IntRem
        | Primitive::IntDiv
        | Primitive::IntMod
        | Primitive::IntAnd
        | Primitive::IntOr
        | Primitive::IntXor
        | Primitive::IntShl
        | Primitive::IntShr
        | Primitive::IntZshr => (Type::I32, Type::I32),
        Primitive::IntEq
        | Primitive::IntNe
        | Primitive::IntLt
        | Primitive::IntLe
        | Primitive::IntGt
        | Primitive::IntGe => (Type::I32, Type::Boolean),
        Primitive::CharEq
        | Primitive::CharNe
        | Primitive::CharLt
        | Primitive::CharLe
        | Primitive::CharGt
        | Primitive::CharGe => (Type::Char, Type::Boolean),
        Primitive::NumberAdd
        | Primitive::NumberSub
        | Primitive::NumberMul
        | Primitive::NumberDiv => (Type::F64, Type::F64),
        Primitive::NumberEq
        | Primitive::NumberNe
        | Primitive::NumberLt
        | Primitive::NumberLe
        | Primitive::NumberGt
        | Primitive::NumberGe => (Type::F64, Type::Boolean),
        Primitive::BooleanAnd
        | Primitive::BooleanOr
        | Primitive::BooleanEq
        | Primitive::BooleanNe => (Type::Boolean, Type::Boolean),
    };
    (
        type_id_for(module, &operand_type),
        type_id_for(module, &result_type),
    )
}

pub(super) fn unary_primitive_types(op: UnaryPrimitive, module: &Module) -> (TypeId, TypeId) {
    let (operand, result) = match op {
        UnaryPrimitive::IntNeg | UnaryPrimitive::IntComplement => (Type::I32, Type::I32),
        UnaryPrimitive::NumberNeg => (Type::F64, Type::F64),
        UnaryPrimitive::BooleanNot => (Type::Boolean, Type::Boolean),
        UnaryPrimitive::IntToNumber => (Type::I32, Type::F64),
        UnaryPrimitive::NumberToInt => (Type::F64, Type::I32),
        UnaryPrimitive::BooleanToInt => (Type::Boolean, Type::I32),
        UnaryPrimitive::IntToBoolean => (Type::I32, Type::Boolean),
        UnaryPrimitive::CharToInt => (Type::Char, Type::I32),
        UnaryPrimitive::IntToChar => (Type::I32, Type::Char),
    };
    (type_id_for(module, &operand), type_id_for(module, &result))
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
    let Type::Record(fields) = module.types.get(id.0 as usize)? else {
        return None;
    };
    fields
        .iter()
        .find(|(field, _)| field == label)
        .map(|(_, id)| *id)
}

pub(super) fn user_type_constructor(mut id: TypeId, module: &Module) -> Option<HirTypeId> {
    loop {
        match module.types.get(id.0 as usize)? {
            Type::Application(function, _) => id = *function,
            Type::Constructor(TypeConstructor::User(id)) => return Some(*id),
            _ => return None,
        }
    }
}

pub(super) fn compatible(
    actual: TypeId,
    expected: TypeId,
    module: &Module,
    owner: ModuleId,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    if !types_compatible(actual, expected, module, &mut HashSet::new()) {
        errors.push(error(
            owner,
            span,
            "Core expression type is inconsistent with its context",
        ));
    }
}

fn types_compatible(
    left: TypeId,
    right: TypeId,
    module: &Module,
    seen: &mut HashSet<(TypeId, TypeId)>,
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
        (Type::Variable(_), _) | (_, Type::Variable(_)) => true,
        (Type::I32, Type::I32)
        | (Type::F64, Type::F64)
        | (Type::Boolean, Type::Boolean)
        | (Type::String, Type::String)
        | (Type::Char, Type::Char)
        | (Type::Unit, Type::Unit) => true,
        (Type::Constructor(a), Type::Constructor(b)) => a == b,
        (Type::Application(a1, a2), Type::Application(b1, b2))
        | (
            Type::Function {
                parameter: a1,
                result: a2,
            },
            Type::Function {
                parameter: b1,
                result: b2,
            },
        ) => types_compatible(*a1, *b1, module, seen) && types_compatible(*a2, *b2, module, seen),
        (Type::Record(a), Type::Record(b)) => {
            a.len() == b.len()
                && a.iter().all(|(label, ty)| {
                    b.iter()
                        .find(|(other, _)| other == label)
                        .is_some_and(|(_, other)| types_compatible(*ty, *other, module, seen))
                })
        }
        _ => false,
    }
}

pub(super) fn restore_local(locals: &mut Locals, id: LocalId, previous: Option<TypeId>) {
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
