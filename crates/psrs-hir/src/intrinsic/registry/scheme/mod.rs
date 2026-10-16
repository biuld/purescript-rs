use super::*;

fn empty_span() -> TextRange {
    TextRange::new(0, 0)
}

fn builtin(builtin: BuiltinType) -> Type {
    Type {
        kind: TypeKind::Constructor(builtin),
        span: empty_span(),
    }
}

fn named(id: TypeId) -> Type {
    Type {
        kind: TypeKind::Named(id),
        span: empty_span(),
    }
}

fn variable(name: &str) -> Type {
    Type {
        kind: TypeKind::Variable(name.to_owned()),
        span: empty_span(),
    }
}

fn apply(function: Type, argument: Type) -> Type {
    Type {
        kind: TypeKind::Application(Box::new(function), Box::new(argument)),
        span: empty_span(),
    }
}

fn arrow(parameter: Type, result: Type) -> Type {
    Type {
        kind: TypeKind::Function {
            parameter: Box::new(parameter),
            result: Box::new(result),
        },
        span: empty_span(),
    }
}

fn forall(names: &[&str], body: Type) -> Type {
    Type {
        kind: TypeKind::Forall {
            variables: names
                .iter()
                .map(|name| TypeParameter {
                    name: (*name).to_owned(),
                    name_span: empty_span(),
                    kind: None,
                })
                .collect(),
            body: Box::new(body),
        },
        span: empty_span(),
    }
}

fn array(element: Type) -> Type {
    apply(builtin(BuiltinType::Array), element)
}

fn int() -> Type {
    builtin(BuiltinType::Int)
}

fn scalar(operand: BuiltinType, result: BuiltinType) -> Type {
    let operand = builtin(operand);
    arrow(operand.clone(), arrow(operand, builtin(result)))
}

fn unary(operand: BuiltinType, result: BuiltinType) -> Type {
    arrow(builtin(operand), builtin(result))
}

pub(super) fn boolean() -> Type {
    builtin(BuiltinType::Boolean)
}

pub(super) fn unit() -> Type {
    builtin(BuiltinType::Unit)
}

pub(super) fn int_int_int() -> Type {
    scalar(BuiltinType::Int, BuiltinType::Int)
}

pub(super) fn int_int_bool() -> Type {
    scalar(BuiltinType::Int, BuiltinType::Boolean)
}

pub(super) fn number_number_number() -> Type {
    scalar(BuiltinType::Number, BuiltinType::Number)
}

pub(super) fn number_number_bool() -> Type {
    scalar(BuiltinType::Number, BuiltinType::Boolean)
}

pub(super) fn boolean_boolean_boolean() -> Type {
    scalar(BuiltinType::Boolean, BuiltinType::Boolean)
}

pub(super) fn char_char_bool() -> Type {
    scalar(BuiltinType::Char, BuiltinType::Boolean)
}

pub(super) fn int_int() -> Type {
    unary(BuiltinType::Int, BuiltinType::Int)
}

pub(super) fn number_number() -> Type {
    unary(BuiltinType::Number, BuiltinType::Number)
}

pub(super) fn number_bool() -> Type {
    unary(BuiltinType::Number, BuiltinType::Boolean)
}

pub(super) fn number() -> Type {
    builtin(BuiltinType::Number)
}

pub(super) fn boolean_boolean() -> Type {
    unary(BuiltinType::Boolean, BuiltinType::Boolean)
}

pub(super) fn int_number() -> Type {
    unary(BuiltinType::Int, BuiltinType::Number)
}

pub(super) fn string_number() -> Type {
    unary(BuiltinType::String, BuiltinType::Number)
}

pub(super) fn number_string() -> Type {
    unary(BuiltinType::Number, BuiltinType::String)
}

pub(super) fn number_int() -> Type {
    unary(BuiltinType::Number, BuiltinType::Int)
}

pub(super) fn boolean_int() -> Type {
    unary(BuiltinType::Boolean, BuiltinType::Int)
}

pub(super) fn int_boolean() -> Type {
    unary(BuiltinType::Int, BuiltinType::Boolean)
}

pub(super) fn char_int() -> Type {
    unary(BuiltinType::Char, BuiltinType::Int)
}

pub(super) fn int_char() -> Type {
    unary(BuiltinType::Int, BuiltinType::Char)
}

pub(super) fn array_length() -> Type {
    forall(&["a"], arrow(array(variable("a")), int()))
}

pub(super) fn array_index() -> Type {
    forall(
        &["a"],
        arrow(array(variable("a")), arrow(int(), variable("a"))),
    )
}

pub(super) fn array_update() -> Type {
    forall(
        &["a"],
        arrow(
            array(variable("a")),
            arrow(int(), arrow(variable("a"), array(variable("a")))),
        ),
    )
}

pub(super) fn array_append() -> Type {
    forall(
        &["a"],
        arrow(
            array(variable("a")),
            arrow(array(variable("a")), array(variable("a"))),
        ),
    )
}

pub(super) fn array_fill() -> Type {
    forall(
        &["a"],
        arrow(int(), arrow(variable("a"), array(variable("a")))),
    )
}

pub(super) fn string_to_bytes() -> Type {
    arrow(builtin(BuiltinType::String), array(int()))
}

pub(super) fn bytes_to_string() -> Type {
    arrow(array(int()), builtin(BuiltinType::String))
}

pub(super) fn undefined() -> Type {
    forall(&["a"], variable("a"))
}

pub(super) fn coerce() -> Type {
    let constraint = apply(
        apply(named(TypeId::COERCIBLE), variable("a")),
        variable("b"),
    );
    Type {
        kind: TypeKind::Constrained {
            constraint: Box::new(constraint),
            body: Box::new(arrow(variable("a"), variable("b"))),
        },
        span: empty_span(),
    }
}

/// `Unsafe.Coerce.unsafeCoerce`: an unconstrained identity cast. It has no
/// `Coercible` proof; the value crosses its erased representation unchanged.
pub(super) fn unsafe_coerce() -> Type {
    forall(&["a", "b"], arrow(variable("a"), variable("b")))
}

mod control;
pub(super) use control::{
    control_bind, control_cancel, control_handle, control_once, control_perform, control_resume,
    control_return, control_run,
};
