//! Narrowed and unsigned WIT integer coverage for ABI-06.

use super::canonical::resolve as canonical_resolve;
use super::test_support::import;
use super::*;
use psrs_core::Type as CoreType;

#[test]
fn classifies_narrow_and_unsigned_wit_integers() {
    let resolve = Resolve::default();
    assert_eq!(
        canonical_resolve(&resolve, &WitType::S32),
        Some(int(32, true))
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::U32),
        Some(int(32, false))
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::U8),
        Some(int(8, false))
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::S8),
        Some(int(8, true))
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::U16),
        Some(int(16, false))
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::S16),
        Some(int(16, true))
    );

    // Every integer maps to source `Int`.
    let import = import(
        psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        "test:integers",
        "take",
        vec![int(8, false)],
        None,
    );
    validate_core(
        &import,
        vec![CoreType::Constructor(psrs_core::TypeConstructor::Int)],
        CoreType::Constructor(psrs_core::TypeConstructor::Unit),
    )
    .expect("a narrowed WIT integer accepts source Int");
    assert!(
        validate_core(
            &import,
            vec![CoreType::Constructor(psrs_core::TypeConstructor::Boolean)],
            CoreType::Constructor(psrs_core::TypeConstructor::Unit)
        )
        .is_err(),
        "a narrowed WIT integer rejects a Boolean"
    );
}

#[test]
fn exact_word64_carrier_requires_two_integer_words_and_a_wide_wit_type() {
    for (width, accepted) in [(32, false), (64, true)] {
        for signed in [false, true] {
            for (labels, second, valid) in [
                (["high", "low"], psrs_core::TypeConstructor::Int, true),
                (["high", "low"], psrs_core::TypeConstructor::Number, false),
                (["high", "other"], psrs_core::TypeConstructor::Int, false),
            ] {
                let mut module = empty_core_module();
                let ids = intern_all(
                    &mut module,
                    vec![
                        CoreType::Constructor(psrs_core::TypeConstructor::Int),
                        CoreType::Constructor(second),
                    ],
                );
                let carrier = push_record(
                    &mut module,
                    vec![(labels[0].into(), ids[0]), (labels[1].into(), ids[1])],
                );
                let descriptor = import(
                    psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
                    "test:integers",
                    "round-trip",
                    vec![int(width, signed)],
                    Some(int(width, signed)),
                );
                assert_eq!(
                    validate_against(&descriptor, module, &[carrier], carrier).is_ok(),
                    accepted && valid,
                    "width={width} signed={signed} labels={labels:?} second={second:?}"
                );
            }
        }
    }
}

#[test]
fn exact_word64_rejects_an_open_record_carrier() {
    let mut module = empty_core_module();
    let integer = intern_all(
        &mut module,
        vec![CoreType::Constructor(psrs_core::TypeConstructor::Int)],
    )[0];
    let carrier = push_record(
        &mut module,
        vec![("high".into(), integer), ("low".into(), integer)],
    );
    let tail = module
        .types
        .iter_mut()
        .find(|ty| matches!(ty, CoreType::RowEmpty))
        .unwrap();
    *tail = CoreType::Variable(psrs_hir::TypeVariableId(0));
    assert_eq!(module.record_is_open(carrier), Some(true));
    let descriptor = import(
        psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        "test:integers",
        "round-trip",
        vec![int(64, false)],
        Some(int(64, false)),
    );
    assert!(validate_against(&descriptor, module, &[carrier], carrier).is_err());
}
