//! Target state facilities are separate from the official PureScript vocabulary.
use super::*;

pub(super) fn declarations() -> Vec<(&'static str, TypeDeclaration)> {
    vec![
        (
            "Prim.State",
            foreign_type(
                TypeId::PRIM_STATE,
                "State",
                forall_kind_result("k", vec![variable("k")], builtin(BuiltinType::Type)),
                &[Role::Nominal],
            ),
        ),
        (
            "Prim.State",
            foreign_type(
                TypeId::PRIM_REAL_WORLD,
                "RealWorld",
                builtin(BuiltinType::Type),
                &[],
            ),
        ),
    ]
}
