use super::super::{Checker, InferType, TypeConstructor};
use psrs_hir::Intrinsic;

impl Checker {
    pub(super) fn intrinsic_type(&mut self, intrinsic: Intrinsic) -> Option<InferType> {
        if intrinsic == Intrinsic::ArrayLength {
            let element = self.fresh();
            return Some(InferType::Function(
                Box::new(InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(element),
                )),
                Box::new(InferType::I32),
            ));
        }
        if intrinsic == Intrinsic::ArrayIndex {
            let element = self.fresh();
            return Some(InferType::Function(
                Box::new(InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(element.clone()),
                )),
                Box::new(InferType::Function(
                    Box::new(InferType::I32),
                    Box::new(element),
                )),
            ));
        }
        if intrinsic == Intrinsic::ArrayUpdate {
            let element = self.fresh();
            let array = InferType::Application(
                Box::new(InferType::Constructor(TypeConstructor::Array)),
                Box::new(element.clone()),
            );
            return Some(InferType::Function(
                Box::new(array.clone()),
                Box::new(InferType::Function(
                    Box::new(InferType::I32),
                    Box::new(InferType::Function(Box::new(element), Box::new(array))),
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
            | Intrinsic::ArrayIndex
            | Intrinsic::ArrayUpdate => return None,
        };
        Some(InferType::Function(
            Box::new(InferType::I32),
            Box::new(InferType::Function(
                Box::new(InferType::I32),
                Box::new(result),
            )),
        ))
    }
}
