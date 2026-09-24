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
        if let Some((operand, result)) = match intrinsic {
            Intrinsic::IntNeg | Intrinsic::IntComplement => Some((InferType::I32, InferType::I32)),
            Intrinsic::NumberNeg => Some((InferType::F64, InferType::F64)),
            Intrinsic::BooleanNot => Some((InferType::Boolean, InferType::Boolean)),
            Intrinsic::IntToNumber => Some((InferType::I32, InferType::F64)),
            Intrinsic::NumberToInt => Some((InferType::F64, InferType::I32)),
            Intrinsic::BooleanToInt => Some((InferType::Boolean, InferType::I32)),
            Intrinsic::IntToBoolean => Some((InferType::I32, InferType::Boolean)),
            Intrinsic::CharToInt => Some((InferType::Char, InferType::I32)),
            Intrinsic::IntToChar => Some((InferType::I32, InferType::Char)),
            _ => None,
        } {
            return Some(curried(vec![operand], result));
        }

        let (operand, result) = match intrinsic {
            Intrinsic::I32Add
            | Intrinsic::I32Sub
            | Intrinsic::I32Mul
            | Intrinsic::I32DivS
            | Intrinsic::I32RemS
            | Intrinsic::IntDiv
            | Intrinsic::IntMod
            | Intrinsic::IntAnd
            | Intrinsic::IntOr
            | Intrinsic::IntXor
            | Intrinsic::IntShl
            | Intrinsic::IntShr
            | Intrinsic::IntZshr => (InferType::I32, InferType::I32),
            Intrinsic::I32Eq
            | Intrinsic::I32Ne
            | Intrinsic::I32LtS
            | Intrinsic::I32LeS
            | Intrinsic::I32GtS
            | Intrinsic::I32GeS => (InferType::I32, InferType::Boolean),
            Intrinsic::NumberAdd
            | Intrinsic::NumberSub
            | Intrinsic::NumberMul
            | Intrinsic::NumberDiv => (InferType::F64, InferType::F64),
            Intrinsic::NumberEq
            | Intrinsic::NumberNe
            | Intrinsic::NumberLt
            | Intrinsic::NumberLe
            | Intrinsic::NumberGt
            | Intrinsic::NumberGe => (InferType::F64, InferType::Boolean),
            Intrinsic::BooleanAnd | Intrinsic::BooleanOr => {
                (InferType::Boolean, InferType::Boolean)
            }
            Intrinsic::BooleanEq | Intrinsic::BooleanNe => (InferType::Boolean, InferType::Boolean),
            Intrinsic::CharEq
            | Intrinsic::CharNe
            | Intrinsic::CharLt
            | Intrinsic::CharLe
            | Intrinsic::CharGt
            | Intrinsic::CharGe => (InferType::Char, InferType::Boolean),
            Intrinsic::BoolTrue
            | Intrinsic::BoolFalse
            | Intrinsic::ArrayLength
            | Intrinsic::ArrayIndex
            | Intrinsic::ArrayUpdate
            | Intrinsic::IntNeg
            | Intrinsic::IntComplement
            | Intrinsic::NumberNeg
            | Intrinsic::BooleanNot
            | Intrinsic::IntToNumber
            | Intrinsic::NumberToInt
            | Intrinsic::BooleanToInt
            | Intrinsic::IntToBoolean
            | Intrinsic::CharToInt
            | Intrinsic::IntToChar => return None,
        };
        Some(curried(vec![operand.clone(), operand], result))
    }
}

fn curried(arguments: Vec<InferType>, result: InferType) -> InferType {
    arguments
        .into_iter()
        .rev()
        .fold(result, |result, argument| {
            InferType::Function(Box::new(argument), Box::new(result))
        })
}
