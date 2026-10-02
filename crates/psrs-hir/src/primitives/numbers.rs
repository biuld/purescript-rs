use super::*;

pub(super) fn declarations() -> Vec<(&'static str, TypeDeclaration)> {
    vec![
        (
            "Prim.Int",
            class(
                TypeId::PRIM_INT_ADD,
                "Add",
                &["left", "right", "sum"],
                function_kind(vec![
                    builtin(BuiltinType::Int),
                    builtin(BuiltinType::Int),
                    builtin(BuiltinType::Int),
                ]),
                vec![
                    fd(&["left", "right"], &["sum"]),
                    fd(&["left", "sum"], &["right"]),
                    fd(&["right", "sum"], &["left"]),
                ],
            ),
        ),
        (
            "Prim.Int",
            class(
                TypeId::PRIM_INT_COMPARE,
                "Compare",
                &["left", "right", "ordering"],
                function_kind(vec![
                    builtin(BuiltinType::Int),
                    builtin(BuiltinType::Int),
                    named(TypeId::PRIM_ORDERING),
                ]),
                vec![fd(&["left", "right"], &["ordering"])],
            ),
        ),
        (
            "Prim.Int",
            class(
                TypeId::PRIM_INT_MUL,
                "Mul",
                &["left", "right", "product"],
                function_kind(vec![
                    builtin(BuiltinType::Int),
                    builtin(BuiltinType::Int),
                    builtin(BuiltinType::Int),
                ]),
                vec![fd(&["left", "right"], &["product"])],
            ),
        ),
        (
            "Prim.Int",
            class(
                TypeId::PRIM_INT_TO_STRING,
                "ToString",
                &["int", "string"],
                function_kind(vec![
                    builtin(BuiltinType::Int),
                    builtin(BuiltinType::Symbol),
                ]),
                vec![fd(&["int"], &["string"])],
            ),
        ),
        (
            "Prim.Symbol",
            class(
                TypeId::PRIM_SYMBOL_APPEND,
                "Append",
                &["left", "right", "appended"],
                function_kind(vec![
                    builtin(BuiltinType::Symbol),
                    builtin(BuiltinType::Symbol),
                    builtin(BuiltinType::Symbol),
                ]),
                vec![
                    fd(&["left", "right"], &["appended"]),
                    fd(&["right", "appended"], &["left"]),
                    fd(&["appended", "left"], &["right"]),
                ],
            ),
        ),
        (
            "Prim.Symbol",
            class(
                TypeId::PRIM_SYMBOL_COMPARE,
                "Compare",
                &["left", "right", "ordering"],
                function_kind(vec![
                    builtin(BuiltinType::Symbol),
                    builtin(BuiltinType::Symbol),
                    named(TypeId::PRIM_ORDERING),
                ]),
                vec![fd(&["left", "right"], &["ordering"])],
            ),
        ),
        (
            "Prim.Symbol",
            class(
                TypeId::PRIM_SYMBOL_CONS,
                "Cons",
                &["head", "tail", "symbol"],
                function_kind(vec![
                    builtin(BuiltinType::Symbol),
                    builtin(BuiltinType::Symbol),
                    builtin(BuiltinType::Symbol),
                ]),
                vec![
                    fd(&["head", "tail"], &["symbol"]),
                    fd(&["symbol"], &["head", "tail"]),
                ],
            ),
        ),
    ]
}
