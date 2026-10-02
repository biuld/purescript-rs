use super::*;

pub(super) fn declarations() -> Vec<(&'static str, TypeDeclaration)> {
    vec![
        (
            "Prim.Row",
            class(
                TypeId::PRIM_ROW_CONS,
                "Cons",
                &["label", "value", "tail", "row"],
                forall_kind(
                    "k",
                    vec![
                        builtin(BuiltinType::Symbol),
                        variable("k"),
                        row_kind(variable("k")),
                        row_kind(variable("k")),
                    ],
                ),
                vec![
                    fd(&["label", "tail", "value"], &["row"]),
                    fd(&["label", "row"], &["tail", "value"]),
                ],
            ),
        ),
        (
            "Prim.Row",
            class(
                TypeId::PRIM_ROW_LACKS,
                "Lacks",
                &["label", "row"],
                forall_kind(
                    "k",
                    vec![builtin(BuiltinType::Symbol), row_kind(variable("k"))],
                ),
                Vec::new(),
            ),
        ),
        (
            "Prim.Row",
            class(
                TypeId::PRIM_ROW_NUB,
                "Nub",
                &["original", "nubbed"],
                forall_kind("k", vec![row_kind(variable("k")), row_kind(variable("k"))]),
                vec![fd(&["original"], &["nubbed"])],
            ),
        ),
        (
            "Prim.Row",
            class(
                TypeId::PRIM_ROW_UNION,
                "Union",
                &["left", "right", "union"],
                forall_kind(
                    "k",
                    vec![
                        row_kind(variable("k")),
                        row_kind(variable("k")),
                        row_kind(variable("k")),
                    ],
                ),
                vec![
                    fd(&["left", "right"], &["union"]),
                    fd(&["right", "union"], &["left"]),
                    fd(&["union", "left"], &["right"]),
                ],
            ),
        ),
        (
            "Prim.RowList",
            foreign_type(
                TypeId::PRIM_ROW_LIST,
                "RowList",
                arrow_kind(vec![builtin(BuiltinType::Type)], builtin(BuiltinType::Type)),
            ),
        ),
        (
            "Prim.RowList",
            foreign_type(
                TypeId::PRIM_ROW_LIST_CONS,
                "Cons",
                forall_kind_result(
                    "k",
                    vec![
                        builtin(BuiltinType::Symbol),
                        variable("k"),
                        apply(named(TypeId::PRIM_ROW_LIST), variable("k")),
                    ],
                    apply(named(TypeId::PRIM_ROW_LIST), variable("k")),
                ),
            ),
        ),
        (
            "Prim.RowList",
            foreign_type(
                TypeId::PRIM_ROW_LIST_NIL,
                "Nil",
                forall_kind_result(
                    "k",
                    vec![apply(named(TypeId::PRIM_ROW_LIST), variable("k"))],
                    apply(named(TypeId::PRIM_ROW_LIST), variable("k")),
                ),
            ),
        ),
        (
            "Prim.RowList",
            class(
                TypeId::PRIM_ROW_TO_LIST,
                "RowToList",
                &["row", "list"],
                forall_kind(
                    "k",
                    vec![
                        row_kind(variable("k")),
                        apply(named(TypeId::PRIM_ROW_LIST), variable("k")),
                    ],
                ),
                vec![fd(&["row"], &["list"])],
            ),
        ),
    ]
}
