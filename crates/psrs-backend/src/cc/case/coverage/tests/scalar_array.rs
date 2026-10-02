use super::super::analyze;
use super::{branch, module, pat};
use psrs_core::{Literal, Pattern, PatternKind, Type, TypeConstructor, TypeId};
use psrs_hir::CaseBranchCoverage;
use psrs_span::TextRange;

fn literal(ty: u32, value: Literal) -> Pattern {
    pat(ty, PatternKind::Literal { value })
}

fn array(ty: u32, elements: Vec<Pattern>) -> Pattern {
    pat(ty, PatternKind::Array { elements })
}

#[test]
fn boolean_literals_form_a_finite_signature_and_duplicates_are_redundant() {
    let module = module(
        vec![Type::Constructor(TypeConstructor::Boolean)],
        Vec::new(),
    );
    let branches = [
        branch(literal(0, Literal::Boolean(false))),
        branch(literal(0, Literal::Boolean(true))),
        branch(literal(0, Literal::Boolean(false))),
    ];
    let report = analyze(&module, TypeId(0), &branches);
    assert!(report.exhaustive, "{report:?}");
    assert_eq!(report.redundant_branches, vec![2]);
}

#[test]
fn guarded_boolean_rows_are_redundant_only_after_unconditional_coverage() {
    let module = module(
        vec![Type::Constructor(TypeConstructor::Boolean)],
        Vec::new(),
    );
    let mut guarded_true = branch(literal(0, Literal::Boolean(true)));
    guarded_true.coverage = CaseBranchCoverage::Guarded;
    let branches = [
        branch(literal(0, Literal::Boolean(true))),
        guarded_true,
        branch(literal(0, Literal::Boolean(false))),
    ];

    let report = analyze(&module, TypeId(0), &branches);

    assert!(report.exhaustive, "{report:?}");
    assert_eq!(report.redundant_branches, vec![1]);
}

#[test]
fn guarded_boolean_rows_do_not_cover_or_make_each_other_redundant() {
    let module = module(
        vec![Type::Constructor(TypeConstructor::Boolean)],
        Vec::new(),
    );
    let mut first_guarded_true = branch(literal(0, Literal::Boolean(true)));
    first_guarded_true.coverage = CaseBranchCoverage::Guarded;
    let mut second_guarded_true = branch(literal(0, Literal::Boolean(true)));
    second_guarded_true.coverage = CaseBranchCoverage::Guarded;
    let branches = [
        first_guarded_true,
        second_guarded_true,
        branch(literal(0, Literal::Boolean(true))),
        branch(literal(0, Literal::Boolean(false))),
    ];

    let report = analyze(&module, TypeId(0), &branches);

    assert!(report.exhaustive, "{report:?}");
    assert!(report.redundant_branches.is_empty(), "{report:?}");
}

#[test]
fn scalar_literals_need_a_wildcard_and_number_keys_follow_numeric_equality() {
    let module = module(vec![Type::Constructor(TypeConstructor::Number)], Vec::new());
    let branches = [
        branch(literal(0, Literal::Number("0.0".into()))),
        branch(literal(0, Literal::Number("-0.0".into()))),
    ];
    let report = analyze(&module, TypeId(0), &branches);
    assert!(!report.exhaustive);
    assert_eq!(report.witness.as_deref(), Some("_"));
    assert_eq!(report.redundant_branches, vec![1]);
}

#[test]
fn array_rows_only_cover_observed_lengths_until_an_irrefutable_row() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(TypeId(1), TypeId(0)),
        ],
        Vec::new(),
    );
    let first = array(2, vec![literal(0, Literal::Integer(1))]);
    let repeated = array(2, vec![literal(0, Literal::Integer(1))]);
    let empty = array(2, Vec::new());
    let partial = analyze(
        &module,
        TypeId(2),
        &[branch(empty), branch(first.clone()), branch(repeated)],
    );
    assert!(!partial.exhaustive);
    assert_eq!(partial.witness.as_deref(), Some("[_, _]"));
    assert_eq!(partial.redundant_branches, vec![2]);

    let complete = analyze(
        &module,
        TypeId(2),
        &[branch(first), branch(pat(2, PatternKind::Wildcard))],
    );
    assert!(complete.exhaustive, "{complete:?}");
}

#[test]
fn malformed_or_mismatched_array_element_types_do_not_panic_coverage() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(TypeId(1), TypeId(0)),
        ],
        Vec::new(),
    );
    let malformed = Pattern {
        kind: PatternKind::Array {
            elements: vec![literal(9, Literal::String("x".into()))],
        },
        ty: TypeId(2),
        span: TextRange::new(0, 1),
    };
    let report = analyze(&module, TypeId(2), &[branch(malformed)]);
    assert!(!report.exhaustive);
}
