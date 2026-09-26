//! Narrowed and unsigned WIT integer coverage for ABI-06.

use super::*;
use psrs_core::Type as CoreType;

#[test]
fn classifies_narrow_and_unsigned_wit_integers() {
    let resolve = Resolve::default();
    assert_eq!(
        param_kind(&resolve, &WitType::S32),
        WasiParamKind::Integer32
    );
    assert_eq!(
        param_kind(&resolve, &WitType::U32),
        WasiParamKind::Integer32
    );
    assert_eq!(
        param_kind(&resolve, &WitType::U8),
        WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: false
        }
    );
    assert_eq!(
        param_kind(&resolve, &WitType::S8),
        WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: true
        }
    );
    assert_eq!(
        param_kind(&resolve, &WitType::U16),
        WasiParamKind::IntegerNarrow {
            bits: 16,
            signed: false
        }
    );
    assert_eq!(
        param_kind(&resolve, &WitType::S16),
        WasiParamKind::IntegerNarrow {
            bits: 16,
            signed: true
        }
    );
    assert_eq!(result_kind(&resolve, &WitType::U32), WasiResultKind::Scalar);
    assert_eq!(
        result_kind(&resolve, &WitType::U8),
        WasiResultKind::IntegerNarrow {
            bits: 8,
            signed: false
        }
    );

    // Every integer maps to source `Int`.
    let import = WasiImport {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        module: "test:integers".into(),
        name: "take".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: false,
        }],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    validate_core(&import, vec![CoreType::I32], CoreType::Unit)
        .expect("a narrowed WIT integer accepts source Int");
    assert!(
        validate_core(&import, vec![CoreType::Boolean], CoreType::Unit).is_err(),
        "a narrowed WIT integer rejects a Boolean"
    );
}
