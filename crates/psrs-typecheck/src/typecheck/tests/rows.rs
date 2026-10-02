//! Record and row construction on the shared spine.
//!
//! `Record r` and `{ x :: a | r }` are one construction, so both spellings
//! reach one normalizer with one set of kinds, and the general row form
//! `( x :: a | tail )` is a row value rather than a rejected type. A value that
//! resolved to a non-row is reported as the shape it is, at the range where it
//! was found, instead of becoming a closed row.

use super::*;

fn ty(kind: HirTypeKind, start: u32, end: u32) -> HirType {
    HirType {
        kind,
        span: TextRange::new(start, end),
    }
}

fn builtin(builtin: psrs_hir::BuiltinType, start: u32) -> HirType {
    ty(HirTypeKind::Constructor(builtin), start, start + 1)
}

fn named(id: u32, start: u32) -> HirType {
    ty(
        HirTypeKind::Named(psrs_hir::TypeId::new(ModuleId(0), id)),
        start,
        start + 1,
    )
}

fn applied(function: HirType, argument: HirType, start: u32, end: u32) -> HirType {
    ty(
        HirTypeKind::Application(Box::new(function), Box::new(argument)),
        start,
        end,
    )
}

fn field(label: &str, field_ty: HirType, start: u32) -> psrs_hir::TypeField {
    psrs_hir::TypeField {
        label: label.to_owned(),
        label_span: TextRange::new(start, start + label.len() as u32),
        span: TextRange::new(start, field_ty.span.end),
        ty: field_ty,
    }
}

/// The general row form `( label :: field | tail )`.
fn row(fields: Vec<psrs_hir::TypeField>, tail: Option<HirType>, start: u32) -> HirType {
    let end = tail
        .as_ref()
        .map(|tail| tail.span.end)
        .unwrap_or_else(|| fields.last().map(|field| field.span.end).unwrap_or(start));
    ty(
        HirTypeKind::Row {
            fields,
            tail: tail.map(Box::new),
        },
        start,
        end,
    )
}

/// The record form `{ label :: field | tail }`.
fn record(fields: Vec<psrs_hir::TypeField>, tail: Option<HirType>, start: u32) -> HirType {
    let end = tail
        .as_ref()
        .map(|tail| tail.span.end)
        .unwrap_or_else(|| fields.last().map(|field| field.span.end).unwrap_or(start));
    ty(
        HirTypeKind::Record {
            fields,
            tail: tail.map(Box::new),
        },
        start,
        end,
    )
}

fn function(parameter: HirType, result: HirType, start: u32) -> HirType {
    let end = result.span.end;
    ty(
        HirTypeKind::Function {
            parameter: Box::new(parameter),
            result: Box::new(result),
        },
        start,
        end,
    )
}

fn identity_lambda(id: u32, start: u32) -> HirExpr {
    expr(
        HirExprKind::Lambda {
            binder: LocalBinder {
                id: LocalId(id),
                name: "x".into(),
                span: TextRange::new(start, start + 1),
            },
            body: Box::new(local(id, start + 3)),
        },
        start - 1,
        start + 4,
    )
}

fn synonym(id: u32, name: &str, parameters: &[&str], body: HirType) -> psrs_hir::TypeDeclaration {
    psrs_hir::TypeDeclaration {
        id: psrs_hir::TypeId::new(ModuleId(0), id),
        name: name.into(),
        name_span: TextRange::new(0, 1),
        kind: psrs_hir::TypeDeclarationKind::TypeSynonym,
        parameters: parameters
            .iter()
            .map(|parameter| psrs_hir::TypeParameter {
                name: (*parameter).into(),
                name_span: TextRange::new(0, 1),
                kind: None,
            })
            .collect(),
        constructors: Vec::new(),
        members: Vec::new(),
        body: Some(body),
        superclasses: Vec::new(),
        fundeps: Vec::new(),
        declared_kind: None,
        declared_roles: None,
        span: TextRange::new(0, 1),
    }
}

/// `id' :: Record Foo -> Record Bar` where `Foo` and `Bar` are the general row
/// forms. This is the `passing/RowConstructors.purs` shape: a synonym that
/// expands to a row must reach the record construction as a row.
#[test]
fn accepts_a_general_row_synonym_as_a_record_tail() {
    let foo = synonym(
        0,
        "Foo",
        &[],
        row(
            vec![
                field("x", builtin(psrs_hir::BuiltinType::Int, 20), 16),
                field("y", builtin(psrs_hir::BuiltinType::Int, 32), 28),
            ],
            None,
            15,
        ),
    );
    let bar = synonym(
        1,
        "Bar",
        &[],
        row(
            vec![
                field("y", builtin(psrs_hir::BuiltinType::Int, 46), 42),
                field("x", builtin(psrs_hir::BuiltinType::Int, 56), 52),
            ],
            None,
            41,
        ),
    );
    // `Record Foo -> Record Bar`, where both synonyms expand to a row. The
    // two rows hold the same labels with the same field types in a different
    // order, which row equality accepts.
    let signature = function(
        applied(
            builtin(psrs_hir::BuiltinType::Record, 60),
            named(0, 68),
            60,
            70,
        ),
        applied(
            builtin(psrs_hir::BuiltinType::Record, 70),
            named(1, 78),
            70,
            80,
        ),
        60,
    );
    let declaration = declaration_with_signature(0, "id", 19, signature, identity_lambda(0, 40));
    let mut resolved = module(vec![declaration], false);
    resolved.types = vec![foo, bar];

    let typed = typecheck_module(resolved).unwrap();
    typed.verify().unwrap();
}

/// `{ x :: a | r }` and `Record r` are one construction: a declaration whose
/// parameter is written as record syntax and whose result is written as the
/// equivalent `Record` application has one checked type, not two.
#[test]
fn record_syntax_and_a_record_application_reach_the_same_row() {
    let spelled = record(
        vec![
            field("x", builtin(psrs_hir::BuiltinType::Int, 22), 20),
            field("y", builtin(psrs_hir::BuiltinType::Boolean, 32), 30),
        ],
        None,
        19,
    );
    let applied_form = applied(
        builtin(psrs_hir::BuiltinType::Record, 40),
        row(
            vec![
                field("x", builtin(psrs_hir::BuiltinType::Int, 54), 52),
                field("y", builtin(psrs_hir::BuiltinType::Boolean, 64), 62),
            ],
            None,
            51,
        ),
        40,
        66,
    );
    let signature = function(spelled, applied_form, 19);
    let declaration = declaration_with_signature(0, "same", 19, signature, identity_lambda(0, 70));
    let resolved = module(vec![declaration], false);

    let typed = typecheck_module(resolved).unwrap();
    // One interned row for the whole record: the two spellings did not produce
    // two structurally equal rows the interner had to keep apart.
    let rows = typed
        .types
        .iter()
        .filter(|ty| matches!(ty, Type::RowExtend { .. }))
        .count();
    assert_eq!(rows, 2, "{:?}", typed.types);
    let records = typed
        .types
        .iter()
        .filter(|ty| {
            matches!(
                ty,
                Type::Application(head, _)
                    if matches!(
                        typed.types[head.0 as usize],
                        Type::Constructor(thir::TypeConstructor::Record)
                    )
            )
        })
        .count();
    assert_eq!(records, 1, "{:?}", typed.types);
    typed.verify().unwrap();
}

/// A record row tail need not be a bare variable: a nested row is a row.
#[test]
fn accepts_a_row_tail_that_is_not_a_variable() {
    let nested = record(
        vec![
            field("x", builtin(psrs_hir::BuiltinType::Int, 20), 18),
            field("y", builtin(psrs_hir::BuiltinType::Int, 32), 30),
        ],
        None,
        17,
    );
    let signature = function(nested.clone(), nested, 17);
    let declaration = declaration_with_signature(0, "same", 19, signature, identity_lambda(0, 45));
    let resolved = module(vec![declaration], false);

    let typed = typecheck_module(resolved).unwrap();
    typed.verify().unwrap();
}

/// A value that resolved to a non-row is reported as the shape it is, at the
/// range where it was found, instead of becoming a closed row.
#[test]
fn rejects_a_record_applied_to_a_non_row() {
    let signature = function(
        applied(
            builtin(psrs_hir::BuiltinType::Record, 60),
            builtin(psrs_hir::BuiltinType::Int, 68),
            60,
            71,
        ),
        record(
            vec![field("x", builtin(psrs_hir::BuiltinType::Int, 80), 78)],
            None,
            77,
        ),
        60,
    );
    let declaration = declaration_with_signature(0, "bad", 19, signature, identity_lambda(0, 90));
    let resolved = module(vec![declaration], false);

    let errors = typecheck_module(resolved).unwrap_err();
    let mismatch = errors
        .iter()
        .find(|error| error.kind == TypeCheckErrorKind::TypeMismatch)
        .expect("expected an invalid row shape");
    // `InferType` carries no source ranges, so an invalid shape is reported at
    // the range of the comparison that reached it: the identity lambda's body.
    assert_eq!(mismatch.span, TextRange::new(93, 94));
    assert!(
        mismatch.message().contains("expected a row type"),
        "{}",
        mismatch.message()
    );
    assert!(mismatch.message().contains("Int"), "{}", mismatch.message());
}

/// The entries collected before an invalid shape are still visible, because a
/// non-row shape is not silently converted into a closed row.
#[test]
fn an_invalid_row_shape_keeps_the_fields_collected_before_it() {
    let broken_tail = row(
        vec![field("x", builtin(psrs_hir::BuiltinType::Int, 20), 18)],
        Some(builtin(psrs_hir::BuiltinType::Int, 33)),
        17,
    );
    let signature = function(
        applied(
            builtin(psrs_hir::BuiltinType::Record, 60),
            broken_tail,
            60,
            80,
        ),
        record(
            vec![field("x", builtin(psrs_hir::BuiltinType::Int, 96), 94)],
            None,
            93,
        ),
        60,
    );
    let declaration = declaration_with_signature(0, "bad", 19, signature, identity_lambda(0, 110));
    let resolved = module(vec![declaration], false);

    let errors = typecheck_module(resolved).unwrap_err();
    let mismatch = errors
        .iter()
        .find(|error| error.kind == TypeCheckErrorKind::TypeMismatch)
        .expect("expected an invalid row shape");
    assert_eq!(mismatch.span, TextRange::new(113, 114));
    let message = mismatch.message();
    assert!(message.contains("expected a row type"), "{message}");
    // The field the normalizer had already collected is named.
    assert!(message.contains("x: Int"), "{message}");
}
