use super::*;

#[test]
fn every_active_identity_has_an_explicit_implementation() {
    for intrinsic in Intrinsic::ALL {
        match implementation(intrinsic) {
            Implementation::Direct(ScalarOperation::Unary(_)) => {
                assert_eq!(intrinsic.descriptor().arity, 1)
            }
            Implementation::Direct(ScalarOperation::Binary(_)) => {
                assert_eq!(intrinsic.descriptor().arity, 2)
            }
            Implementation::Artifact(provider) => {
                assert_eq!(provider.intrinsic, intrinsic);
                provider.validate_protocol().unwrap();
                assert_eq!(
                    provider.language_signature().unwrap().0.len(),
                    intrinsic.descriptor().arity as usize
                );
            }
            Implementation::Unsupported => assert_eq!(intrinsic, Intrinsic::Undefined),
            Implementation::Generated(_) | Implementation::Elaborated => {}
        }
    }
    assert!(Intrinsic::from_binding("intDiv").is_none());
    assert!(Intrinsic::from_binding("intMod").is_none());
}

#[test]
fn language_renaming_preserves_stable_symbols_and_retired_slots() {
    for (intrinsic, id) in [
        (Intrinsic::IntAdd, 2),
        (Intrinsic::IntQuot, 5),
        (Intrinsic::IntRem, 6),
        (Intrinsic::IntToChar, 25),
        (Intrinsic::IntAnd, 28),
        (Intrinsic::NumberAbs, 68),
        (Intrinsic::NumberSqrt, 69),
        (Intrinsic::NumberAcos, 70),
        (Intrinsic::NumberAsin, 71),
        (Intrinsic::NumberAtan, 72),
        (Intrinsic::NumberAtan2, 73),
    ] {
        assert_eq!(intrinsic.symbol().index, id);
    }
    for intrinsic in Intrinsic::ALL {
        assert!(!Intrinsic::RESERVED_IDS.contains(&(intrinsic as u32)));
    }
}
