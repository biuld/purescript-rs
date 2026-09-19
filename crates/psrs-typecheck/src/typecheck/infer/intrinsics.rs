use super::super::{InferType, TypeConstructor};
use psrs_hir::Intrinsic;

pub(super) fn intrinsic_type(intrinsic: Intrinsic) -> Option<InferType> {
    if intrinsic == Intrinsic::ArrayLength {
        return Some(InferType::Function(
            Box::new(InferType::Application(
                Box::new(InferType::Constructor(TypeConstructor::Array)),
                Box::new(InferType::I32),
            )),
            Box::new(InferType::I32),
        ));
    }
    if intrinsic == Intrinsic::ArrayIndex {
        return Some(InferType::Function(
            Box::new(InferType::Application(
                Box::new(InferType::Constructor(TypeConstructor::Array)),
                Box::new(InferType::I32),
            )),
            Box::new(InferType::Function(
                Box::new(InferType::I32),
                Box::new(InferType::I32),
            )),
        ));
    }
    let result = match intrinsic {
        Intrinsic::I32Add
        | Intrinsic::I32Sub
        | Intrinsic::I32Mul
        | Intrinsic::I32DivS
        | Intrinsic::I32RemS => InferType::I32,
        Intrinsic::I32Eq
        | Intrinsic::I32Ne
        | Intrinsic::I32LtS
        | Intrinsic::I32LeS
        | Intrinsic::I32GtS
        | Intrinsic::I32GeS => InferType::Boolean,
        Intrinsic::BoolTrue
        | Intrinsic::BoolFalse
        | Intrinsic::ArrayLength
        | Intrinsic::ArrayIndex => return None,
    };
    Some(InferType::Function(
        Box::new(InferType::I32),
        Box::new(InferType::Function(
            Box::new(InferType::I32),
            Box::new(result),
        )),
    ))
}
