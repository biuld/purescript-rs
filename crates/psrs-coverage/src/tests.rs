//! Direct engine tests over hand-built rows, mirroring the backend coverage
//! suite's behavioral anchors.

use crate::{Branch, ConstructorRow, Literal, Pattern, Types, analyze};
use psrs_hir::{CaseBranchCoverage, ModuleId, SymbolId, TypeId};

#[derive(Default)]
struct Fixture {
    constructors: std::collections::HashMap<u32, Vec<ConstructorRow<TypeId>>>,
    booleans: std::collections::HashSet<u32>,
}

impl Fixture {
    fn user_type(index: u32) -> TypeId {
        TypeId::new(ModuleId(0), index)
    }

    fn row(&mut self, type_index: u32, symbol: u32, fields: &[u32]) {
        self.constructors
            .entry(type_index)
            .or_default()
            .push(ConstructorRow {
                symbol: SymbolId::new(ModuleId(0), symbol),
                fields: fields.iter().map(|&f| Self::user_type(f)).collect(),
            });
    }

    fn any(&self) -> Pattern<TypeId> {
        Pattern::Any {
            ty: Self::user_type(0),
        }
    }

    fn constructor(&self, symbol: u32, arguments: Vec<Pattern<TypeId>>) -> Pattern<TypeId> {
        Pattern::Constructor {
            symbol: SymbolId::new(ModuleId(0), symbol),
            arguments,
            ty: Self::user_type(0),
        }
    }

    fn literal(&self, value: bool) -> Pattern<TypeId> {
        Pattern::Literal {
            literal: Literal::Boolean(value),
            ty: Self::user_type(0),
        }
    }
}

impl Types<TypeId> for Fixture {
    fn shape(&self, ty: TypeId) -> TypeId {
        ty
    }

    fn array_element(&self, _ty: TypeId) -> Option<TypeId> {
        None
    }

    fn constructors_of(&self, ty: TypeId) -> Option<Vec<ConstructorRow<TypeId>>> {
        self.constructors.get(&ty.index).cloned()
    }

    fn record_fields(&self, _ty: TypeId) -> Option<Vec<(String, TypeId)>> {
        None
    }

    fn boolean_type(&self, ty: TypeId) -> bool {
        self.booleans.contains(&ty.index)
    }

    fn constructor_name(&self, symbol: SymbolId) -> String {
        format!("C{index}", index = symbol.index)
    }
}

#[test]
fn a_recursive_product_reports_its_missing_nullary_constructor() {
    let mut fixture = Fixture::default();
    fixture.row(0, 0, &[1, 0, 0]);
    fixture.row(0, 1, &[]);
    let node = fixture.constructor(0, vec![fixture.any(), fixture.any(), fixture.any()]);
    let analysis = analyze(&fixture, Fixture::user_type(0), &[source(node)]);
    assert!(!analysis.exhaustive, "{analysis:?}");
    assert_eq!(
        analysis.first_witness_atom(),
        Some("C1"),
        "the nullary constructor is the remaining witness"
    );
    assert!(analysis.redundant_branches.is_empty());
}

#[test]
fn boolean_rows_cover_in_pairs_and_a_repeat_row_is_redundant() {
    let mut fixture = Fixture::default();
    fixture.booleans.insert(0);
    let analysis = analyze(
        &fixture,
        Fixture::user_type(0),
        &[
            source(fixture.literal(true)),
            source(fixture.literal(true)),
            source(fixture.literal(false)),
        ],
    );
    assert!(analysis.exhaustive, "{analysis:?}");
    assert_eq!(analysis.redundant_branches, vec![1]);
}

#[test]
fn guarded_rows_neither_prove_exhaustiveness_nor_make_each_other_redundant() {
    let mut fixture = Fixture::default();
    fixture.booleans.insert(0);
    let analysis = analyze(
        &fixture,
        Fixture::user_type(0),
        &[
            Branch {
                coverage: CaseBranchCoverage::Guarded,
                pattern: fixture.literal(true),
            },
            Branch {
                coverage: CaseBranchCoverage::Guarded,
                pattern: fixture.literal(false),
            },
        ],
    );
    assert!(!analysis.exhaustive, "guards may fail, so no row covers");
    assert!(analysis.redundant_branches.is_empty());
    assert_eq!(
        analysis.first_witness_atom(),
        Some("false"),
        "the uncovered alternates surface as the witness of the guard-gated case"
    );
}

/// Shorthand for the ubiquitous source-class alternative.
fn source(pattern: Pattern<TypeId>) -> Branch<TypeId> {
    Branch {
        coverage: CaseBranchCoverage::Source,
        pattern,
    }
}
