use super::*;
use psrs_hir::Intrinsic;

#[test]
fn primitive_mapping_covers_the_scalar_intrinsic_set() {
    assert_eq!(
        Primitive::from_intrinsic(Intrinsic::I32Add),
        Some(Primitive::IntAdd)
    );
    assert_eq!(
        Primitive::from_intrinsic(Intrinsic::NumberAdd),
        Some(Primitive::NumberAdd)
    );
    assert_eq!(
        UnaryPrimitive::from_intrinsic(Intrinsic::NumberToInt),
        Some(UnaryPrimitive::NumberToInt)
    );
    assert_eq!(Primitive::from_intrinsic(Intrinsic::BoolTrue), None);
}
