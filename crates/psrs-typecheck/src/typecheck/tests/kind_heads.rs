//! Kinds named in a type position.
//!
//! `Type`, `Constraint`, and `Symbol` are kinds, and official PureScript
//! declares each of them with kind `Type` (`Environment.hs`'s `primTypes`). A
//! type position that names one is therefore an ordinary nominal type on the
//! same spine as `Record` and `Row`: it unifies only with itself, and it reaches
//! THIR and Core as a spine head rather than as a rejection. These cases check
//! that each head elaborates, reaches the checked type table, and is kept
//! distinct from the others.

use super::*;

fn ty(kind: HirTypeKind, start: u32, end: u32) -> HirType {
    HirType {
        kind,
        span: TextRange::new(start, end),
    }
}

fn kind_head(builtin: psrs_hir::BuiltinType, text: &str, start: u32) -> HirType {
    ty(
        HirTypeKind::Constructor(builtin),
        start,
        start + text.len() as u32,
    )
}

fn on_kind(builtin: psrs_hir::BuiltinType, text: &str) -> HirType {
    let start = 19;
    ty(
        HirTypeKind::Function {
            parameter: Box::new(kind_head(builtin, text, start)),
            result: Box::new(kind_head(
                hir::BuiltinType::Int,
                "Int",
                start + text.len() as u32 + 4,
            )),
        },
        start,
        start + text.len() as u32 + 7,
    )
}

/// `_` ignored: the declaration's type is `<kind> -> Int`, so the body never
/// needs a value of the named kind.
fn ignore_lambda(index: u32, start: u32) -> HirExpr {
    expr(
        HirExprKind::Lambda {
            binder: LocalBinder {
                id: LocalId(index),
                name: "value".into(),
                span: TextRange::new(start, start + 5),
            },
            body: Box::new(integer("0", start + 9)),
        },
        start,
        start + 10,
    )
}

#[test]
fn accepts_each_kind_named_in_a_type_position() {
    let heads = [
        (psrs_hir::BuiltinType::Type, "Type"),
        (psrs_hir::BuiltinType::Constraint, "Constraint"),
        (psrs_hir::BuiltinType::Symbol, "Symbol"),
    ];
    let declarations = heads
        .iter()
        .enumerate()
        .map(|(index, (builtin, text))| {
            declaration_with_signature(
                index as u32,
                &format!("on{text}"),
                19,
                on_kind(*builtin, text),
                ignore_lambda(index as u32, 40 + index as u32 * 30),
            )
        })
        .collect::<Vec<_>>();

    let typed = typecheck_module(module(declarations, false)).unwrap();
    for constructor in [
        psrs_thir::TypeConstructor::Type,
        psrs_thir::TypeConstructor::Constraint,
        psrs_thir::TypeConstructor::Symbol,
    ] {
        assert!(
            typed.types.iter().any(|ty| matches!(
                ty,
                Type::Constructor(found) if *found == constructor
            )),
            "{constructor:?} should reach the checked type table: {:?}",
            typed.types
        );
    }
    typed.verify().unwrap();
}

#[test]
fn keeps_the_three_kind_heads_apart() {
    let on_type = declaration_with_signature(
        0,
        "onType",
        19,
        on_kind(psrs_hir::BuiltinType::Type, "Type"),
        ignore_lambda(0, 40),
    );
    // `onType (0 :: Constraint)`: naming `Constraint` produces a real nominal
    // type, so an `Int` value is not one and the ascription fails rather than
    // accepting whatever `Type` happens to unify with.
    let call = declaration_with_signature(
        1,
        "call",
        19,
        ty(HirTypeKind::Constructor(hir::BuiltinType::Int), 25, 28),
        expr(
            HirExprKind::Application(
                Box::new(expr(
                    HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                    31,
                    37,
                )),
                Box::new(expr(
                    HirExprKind::Typed {
                        expression: Box::new(integer("0", 40)),
                        ty: kind_head(hir::BuiltinType::Constraint, "Constraint", 44),
                    },
                    39,
                    55,
                )),
            ),
            31,
            55,
        ),
    );

    let errors = typecheck_module(module(vec![on_type, call], false)).unwrap_err();
    assert!(
        errors.iter().any(|error| {
            error.kind == TypeCheckErrorKind::TypeMismatch
                && error.message().contains("expected Constraint, found Int")
        }),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn rejects_a_value_where_a_kind_is_named() {
    let declaration = declaration_with_signature(
        0,
        "onType",
        19,
        on_kind(psrs_hir::BuiltinType::Type, "Type"),
        expr(
            HirExprKind::Application(
                Box::new(expr(
                    HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                    40,
                    45,
                )),
                Box::new(integer("1", 47)),
            ),
            40,
            48,
        ),
    );

    let errors = typecheck_module(module(vec![declaration], false)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "unexpected diagnostics: {errors:?}"
    );
}
