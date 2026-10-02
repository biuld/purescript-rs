use super::*;

pub(super) fn declarations() -> Vec<(&'static str, TypeDeclaration)> {
    vec![
        (
            "Prim",
            class(
                TypeId::PRIM_PARTIAL,
                "Partial",
                &[],
                builtin(BuiltinType::Constraint),
                Vec::new(),
            ),
        ),
        (
            "Prim.Boolean",
            foreign_type(
                TypeId::PRIM_BOOLEAN_FALSE,
                "False",
                builtin(BuiltinType::Type),
            ),
        ),
        (
            "Prim.Boolean",
            foreign_type(
                TypeId::PRIM_BOOLEAN_TRUE,
                "True",
                builtin(BuiltinType::Type),
            ),
        ),
        (
            "Prim.Coerce",
            class(
                TypeId::COERCIBLE,
                "Coercible",
                &["a", "b"],
                forall_kind("k", vec![variable("k"), variable("k")]),
                Vec::new(),
            ),
        ),
        (
            "Prim.Ordering",
            foreign_type(
                TypeId::PRIM_ORDERING,
                "Ordering",
                builtin(BuiltinType::Type),
            ),
        ),
        (
            "Prim.Ordering",
            foreign_type(TypeId::PRIM_ORDERING_LT, "LT", named(TypeId::PRIM_ORDERING)),
        ),
        (
            "Prim.Ordering",
            foreign_type(TypeId::PRIM_ORDERING_EQ, "EQ", named(TypeId::PRIM_ORDERING)),
        ),
        (
            "Prim.Ordering",
            foreign_type(TypeId::PRIM_ORDERING_GT, "GT", named(TypeId::PRIM_ORDERING)),
        ),
    ]
}
