use super::*;
use psrs_core::{CaseBranch, Expr, ExprKind};
use psrs_hir::{LocalId, ModuleId};
use psrs_span::TextRange;
use std::rc::Rc;

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

#[derive(Clone, Debug)]
enum Rec {
    Nil,
    Any,
    Cons(Rc<Rec>),
}

fn recursive_module() -> Module {
    module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::I32,
        ],
        vec![
            constructor(0, "Cons", 0, vec![TypeId(0)]),
            constructor(1, "Nil", 0, Vec::new()),
        ],
    )
}

fn rec_pattern(rec: Rec) -> Pattern {
    match rec {
        Rec::Any => pat(0, PatternKind::Wildcard),
        Rec::Nil => nullary(1, 0),
        Rec::Cons(inner) => pat(
            0,
            PatternKind::Constructor {
                symbol: symbol(0),
                arguments: vec![rec_pattern((*inner).clone())],
            },
        ),
    }
}

fn rec_trees(depth: usize) -> Vec<Rec> {
    if depth == 0 {
        return vec![Rec::Nil];
    }
    let mut trees = vec![Rec::Nil];
    for inner in rec_trees(depth - 1) {
        trees.push(Rec::Cons(Rc::new(inner)));
    }
    trees
}

fn rec_matches(pattern: Rec, tree: Rec) -> bool {
    match pattern {
        Rec::Any => true,
        Rec::Nil => matches!(tree, Rec::Nil),
        Rec::Cons(inner) => {
            matches!(tree, Rec::Cons(tree_inner) if rec_matches((*inner).clone(), (*tree_inner).clone()))
        }
    }
}

fn rec_patterns() -> Vec<Rec> {
    let nil = Rec::Nil;
    let any = Rec::Any;
    let cons_nil = Rec::Cons(Rc::new(Rec::Nil));
    let cons_any = Rec::Cons(Rc::new(Rec::Any));
    let cons_cons_nil = Rec::Cons(Rc::new(Rec::Cons(Rc::new(Rec::Nil))));
    let cons_cons_any = Rec::Cons(Rc::new(Rec::Cons(Rc::new(Rec::Any))));
    vec![nil, any, cons_nil, cons_any, cons_cons_nil, cons_cons_any]
}

#[test]
fn recursive_coverage_agrees_with_a_bounded_first_match_oracle() {
    let module = recursive_module();
    let patterns = rec_patterns();
    let trees = rec_trees(3);
    let mut matrices = Vec::new();
    for first in &patterns {
        matrices.push(vec![first.clone()]);
        for second in &patterns {
            matrices.push(vec![first.clone(), second.clone()]);
            for third in &patterns {
                matrices.push(vec![first.clone(), second.clone(), third.clone()]);
            }
        }
    }
    for matrix in matrices {
        let branches = matrix
            .iter()
            .map(|rec| branch(rec_pattern(rec.clone())))
            .collect::<Vec<_>>();
        let report = analyze(&module, TypeId(0), &branches);
        let covered = trees.iter().all(|tree| {
            matrix
                .iter()
                .any(|pattern| rec_matches(pattern.clone(), tree.clone()))
        });
        assert_eq!(
            report.exhaustive, covered,
            "exhaustiveness disagreed for {matrix:?}"
        );
        let mut redundant = Vec::new();
        for index in 0..matrix.len() {
            let useful = trees.iter().any(|tree| {
                rec_matches(matrix[index].clone(), tree.clone())
                    && !matrix[..index]
                        .iter()
                        .any(|prior| rec_matches(prior.clone(), tree.clone()))
            });
            if !useful {
                redundant.push(index);
            }
        }
        assert_eq!(
            report.redundant_branches, redundant,
            "redundancy disagreed for {matrix:?}"
        );
    }
}
