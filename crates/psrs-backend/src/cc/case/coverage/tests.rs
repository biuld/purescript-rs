use super::*;
use psrs_core::{CaseBranch, Expr, ExprKind};
use psrs_hir::{LocalId, ModuleId};
use psrs_span::TextRange;

fn symbol(index: u32) -> SymbolId {
    SymbolId::new(ModuleId(0), index)
}

fn hir_type_id(index: u32) -> HirTypeId {
    HirTypeId::new(ModuleId(0), index)
}

fn module(types: Vec<Type>, constructors: Vec<psrs_core::ConstructorInfo>) -> Module {
    Module {
        id: ModuleId(0),
        name: "CoverageTest".to_owned(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        constructors,
        declarations: Vec::new(),
        entry: None,
        span: TextRange::default(),
    }
}

fn constructor(
    index: u32,
    name: &str,
    type_id: u32,
    field_types: Vec<TypeId>,
) -> psrs_core::ConstructorInfo {
    psrs_core::ConstructorInfo {
        symbol: symbol(index),
        name: name.to_owned(),
        type_id: hir_type_id(type_id),
        tag: index,
        field_count: field_types.len(),
        field_types,
    }
}

fn pat(ty: u32, kind: PatternKind) -> Pattern {
    Pattern {
        kind,
        ty: TypeId(ty),
        span: TextRange::new(0, 1),
    }
}

fn branch(pattern: Pattern) -> CaseBranch {
    CaseBranch {
        pattern,
        value: Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(2),
            span: TextRange::new(0, 1),
        },
        span: TextRange::new(0, 1),
    }
}

fn nullary(symbol_index: u32, ty: u32) -> Pattern {
    pat(
        ty,
        PatternKind::Constructor {
            symbol: symbol(symbol_index),
            arguments: Vec::new(),
        },
    )
}

fn binder(ty: u32) -> Pattern {
    pat(
        ty,
        PatternKind::Var {
            id: LocalId(0),
            ty: TypeId(ty),
        },
    )
}

#[test]
fn reports_the_missing_nullary_constructor() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::I32,
        ],
        vec![
            constructor(0, "Red", 0, Vec::new()),
            constructor(1, "Blue", 0, Vec::new()),
        ],
    );
    let report = analyze(&module, TypeId(0), &[branch(nullary(0, 0))]);
    assert!(!report.exhaustive);
    assert_eq!(report.witness.as_deref(), Some("Blue"));
}

#[test]
fn recognizes_exhaustive_nested_constructor_patterns() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::Constructor(TypeConstructor::User(hir_type_id(1))),
            Type::I32,
        ],
        vec![
            constructor(0, "Wrap", 0, vec![TypeId(1)]),
            constructor(1, "First", 1, Vec::new()),
            constructor(2, "Second", 1, Vec::new()),
        ],
    );
    let branches = [
        branch(pat(
            0,
            PatternKind::Constructor {
                symbol: symbol(0),
                arguments: vec![nullary(1, 1)],
            },
        )),
        branch(pat(
            0,
            PatternKind::Constructor {
                symbol: symbol(0),
                arguments: vec![nullary(2, 1)],
            },
        )),
    ];
    let report = analyze(&module, TypeId(0), &branches);
    assert!(report.exhaustive, "{report:?}");
    assert!(report.witness.is_none());
}

#[test]
fn reports_a_nested_missing_constructor() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::Constructor(TypeConstructor::User(hir_type_id(1))),
            Type::I32,
        ],
        vec![
            constructor(0, "Wrap", 0, vec![TypeId(1)]),
            constructor(1, "First", 1, Vec::new()),
            constructor(2, "Second", 1, Vec::new()),
        ],
    );
    let branches = [branch(pat(
        0,
        PatternKind::Constructor {
            symbol: symbol(0),
            arguments: vec![nullary(1, 1)],
        },
    ))];
    let report = analyze(&module, TypeId(0), &branches);
    assert_eq!(report.witness.as_deref(), Some("Wrap Second"));
}

#[test]
fn record_products_are_covered_fieldwise_and_duplicate_rows_are_redundant() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::Record(vec![("color".to_owned(), TypeId(0))]),
            Type::I32,
        ],
        vec![
            constructor(0, "Red", 0, Vec::new()),
            constructor(1, "Blue", 0, Vec::new()),
        ],
    );
    let record = |inner| {
        branch(pat(
            1,
            PatternKind::Record {
                fields: vec![("color".to_owned(), inner)],
            },
        ))
    };
    let complete = analyze(
        &module,
        TypeId(1),
        &[record(nullary(0, 0)), record(nullary(1, 0))],
    );
    assert!(complete.exhaustive, "{complete:?}");

    let redundant = analyze(
        &module,
        TypeId(0),
        &[branch(binder(0)), branch(nullary(0, 0))],
    );
    assert_eq!(redundant.redundant_branches, vec![1]);
}

#[test]
fn recursive_adt_wildcard_coverage_terminates() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::I32,
        ],
        vec![
            constructor(0, "Cons", 0, vec![TypeId(0)]),
            constructor(1, "Nil", 0, Vec::new()),
        ],
    );
    let report = analyze(&module, TypeId(0), &[branch(pat(0, PatternKind::Wildcard))]);
    assert!(report.exhaustive, "{report:?}");
}

#[test]
fn recursive_adt_analysis_finds_a_finite_uncovered_witness() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::I32,
        ],
        vec![
            constructor(0, "Cons", 0, vec![TypeId(0)]),
            constructor(1, "Nil", 0, Vec::new()),
        ],
    );
    let cons_nil = branch(pat(
        0,
        PatternKind::Constructor {
            symbol: symbol(0),
            arguments: vec![nullary(1, 0)],
        },
    ));
    let report = analyze(&module, TypeId(0), &[cons_nil]);
    assert_eq!(report.witness.as_deref(), Some("Cons (Cons Nil)"));
}
