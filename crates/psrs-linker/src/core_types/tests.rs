use super::*;

fn signature(source: &str, index: u32) -> CoreSignature {
    CoreTypes::from_module(&wat::parse_str(source).unwrap())
        .unwrap()
        .signature(index)
        .unwrap()
}

#[test]
fn gc_contracts_ignore_module_index_offsets() {
    let a = signature(
        "(module (type (array (mut eqref))) (type (func (param (ref 0) i32) (result eqref))))",
        1,
    );
    let b = signature(
        "(module (type (struct (field i64))) (type (array (mut eqref))) (type (func (param (ref 1) i32) (result eqref))))",
        2,
    );
    assert_eq!(a, b);
}

#[test]
fn gc_contracts_preserve_mutability_nullability_and_group_identity() {
    let base = signature(
        "(module (type (array (mut eqref))) (type (func (param (ref 0)))))",
        1,
    );
    for source in [
        "(module (type (array eqref)) (type (func (param (ref 0)))))",
        "(module (type (array (mut eqref))) (type (func (param (ref null 0)))))",
        "(module (rec (type (array (mut eqref))) (type (struct (field i64)))) (type (func (param (ref 0)))))",
    ] {
        assert_ne!(
            base,
            signature(source, if source.contains("(rec") { 2 } else { 1 })
        );
    }
}

#[test]
fn recursive_contracts_close_group_binders_without_module_indices() {
    let a = signature(
        "(module (rec (type (struct (field (ref null 1)))) (type (array (mut (ref null 0))))) (type (func (param (ref 0)))))",
        2,
    );
    let b = signature(
        "(module (type (array i32)) (rec (type (struct (field (ref null 2)))) (type (array (mut (ref null 1))))) (type (func (param (ref 1)))))",
        3,
    );
    assert_eq!(a, b);
}

#[test]
fn non_function_and_multiple_results_are_explicit_errors() {
    let bytes =
        wat::parse_str("(module (type (array i32)) (type (func (result i32 i64))))").unwrap();
    let types = CoreTypes::from_module(&bytes).unwrap();
    assert!(types.signature(0).unwrap_err().contains("non-function"));
    assert!(types.signature(1).unwrap_err().contains("multiple results"));
}
