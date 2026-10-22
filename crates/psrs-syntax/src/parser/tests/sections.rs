use super::*;

#[test]
fn a_section_hole_is_only_an_immediate_operand() {
    let module = parse("module Main where\nkept = (_ + 2 * 3)\nrejected = (_ * 4 + 1)\n").unwrap();

    let kept = plain_value(as_value(&module.declarations[0]));
    let ExprKind::OperatorSection { operand, side, .. } = &kept.kind else {
        panic!("expected a right section, found {kept:?}");
    };
    assert!(matches!(side, psrs_cst::OperatorSectionSide::Right));
    let ExprKind::Operator { operator, .. } = &operand.kind else {
        panic!("the operand should keep the tighter operator, found {operand:?}");
    };
    assert_eq!(operator.text, "*");

    let rejected = plain_value(as_value(&module.declarations[1]));
    let ExprKind::Parens { expression, .. } = &rejected.kind else {
        panic!("a hole that is not an immediate operand stays parenthesized, found {rejected:?}");
    };
    assert!(
        !matches!(expression.kind, ExprKind::OperatorSection { .. }),
        "{expression:?}"
    );
}
