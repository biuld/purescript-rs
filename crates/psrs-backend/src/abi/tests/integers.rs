//! Narrowed and unsigned WIT integer coverage for ABI-06.

use super::*;

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
    assert!(source_parameter_matches(
        &SourceType::Int,
        &WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: false
        }
    ));
    assert!(!source_parameter_matches(
        &SourceType::Boolean,
        &WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: false
        }
    ));
}
