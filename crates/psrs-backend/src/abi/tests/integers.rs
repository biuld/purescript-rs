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
    validate_core(&import, vec![CoreType::I32], CoreType::Unit)
        .expect("a narrowed WIT integer accepts source Int");
    assert!(
        validate_core(&import, vec![CoreType::Boolean], CoreType::Unit).is_err(),
        "a narrowed WIT integer rejects a Boolean"
    );
}
