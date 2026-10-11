use super::*;

/// The `Prim.Control` interface's carrier type. `Control f a` is an opaque
/// computation over the request functor `f` that yields an `a`, possibly after
/// suspending at `perform`. Its representation is owned by the control lowering;
/// this declaration fixes only its identity, kind, and roles.
pub(super) fn declarations() -> Vec<(&'static str, TypeDeclaration)> {
    vec![
        (
            "Prim.Control",
            foreign_type(
                TypeId::CONTROL,
                "Control",
                arrow_kind(
                    vec![
                        arrow_kind(vec![builtin(BuiltinType::Type)], builtin(BuiltinType::Type)),
                        builtin(BuiltinType::Type),
                    ],
                    builtin(BuiltinType::Type),
                ),
                &[Role::Representational, Role::Representational],
            ),
        ),
        (
            "Prim.Control",
            foreign_type(
                TypeId::RESUMPTION,
                "Resumption",
                arrow_kind(
                    vec![
                        arrow_kind(vec![builtin(BuiltinType::Type)], builtin(BuiltinType::Type)),
                        builtin(BuiltinType::Type),
                        builtin(BuiltinType::Type),
                    ],
                    builtin(BuiltinType::Type),
                ),
                &[Role::Nominal, Role::Nominal, Role::Nominal],
            ),
        ),
    ]
}
