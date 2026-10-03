use super::super::{Checker, ClassConstraint, InferType, InferredExprKind, TypeConstructor, arrow};
use psrs_hir::Intrinsic;
use psrs_span::TextRange;

impl Checker {
    pub(super) fn coercion_function(&mut self, span: TextRange) -> (InferredExprKind, InferType) {
        let source = self.fresh();
        let target = self.fresh();
        let constraint = ClassConstraint {
            class_id: psrs_hir::TypeId::COERCIBLE,
            arguments: vec![source.clone(), target.clone()],
            span,
        };
        let dictionary_type = self.dictionary_type(&constraint);
        let wanted = self.push_wanted(constraint, dictionary_type);
        (
            InferredExprKind::CoerceFunction {
                wanted,
                source: source.clone(),
                target: target.clone(),
            },
            arrow(source, target),
        )
    }

    pub(super) fn intrinsic_type(&mut self, intrinsic: Intrinsic) -> Option<InferType> {
        // `Prim.undefined` is `forall a. a`, so its type is one fresh variable
        // that the use decides. It takes no argument, so there is no arrow to
        // build.
        if intrinsic == Intrinsic::Undefined {
            return Some(self.fresh());
        }
        if intrinsic == Intrinsic::ArrayLength {
            let element = self.fresh();
            return Some(arrow(
                InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(element),
                ),
                primitive(TypeConstructor::Int),
            ));
        }
        if intrinsic == Intrinsic::ArrayIndex {
            let element = self.fresh();
            return Some(arrow(
                InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(element.clone()),
                ),
                arrow(primitive(TypeConstructor::Int), element),
            ));
        }
        if intrinsic == Intrinsic::ArrayUpdate {
            let element = self.fresh();
            let array = InferType::Application(
                Box::new(InferType::Constructor(TypeConstructor::Array)),
                Box::new(element.clone()),
            );
            return Some(arrow(
                array.clone(),
                arrow(primitive(TypeConstructor::Int), arrow(element, array)),
            ));
        }
        if intrinsic == Intrinsic::StringToBytes {
            return Some(arrow(
                primitive(TypeConstructor::String),
                InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(primitive(TypeConstructor::Int)),
                ),
            ));
        }
        if intrinsic == Intrinsic::BytesToString {
            return Some(arrow(
                InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(primitive(TypeConstructor::Int)),
                ),
                primitive(TypeConstructor::String),
            ));
        }
        if let Some((operand, result)) = match intrinsic {
            Intrinsic::IntNeg | Intrinsic::IntComplement => {
                Some((TypeConstructor::Int, TypeConstructor::Int))
            }
            Intrinsic::NumberNeg => Some((TypeConstructor::Number, TypeConstructor::Number)),
            Intrinsic::BooleanNot => Some((TypeConstructor::Boolean, TypeConstructor::Boolean)),
            Intrinsic::IntToNumber => Some((TypeConstructor::Int, TypeConstructor::Number)),
            Intrinsic::NumberToInt => Some((TypeConstructor::Number, TypeConstructor::Int)),
            Intrinsic::BooleanToInt => Some((TypeConstructor::Boolean, TypeConstructor::Int)),
            Intrinsic::IntToBoolean => Some((TypeConstructor::Int, TypeConstructor::Boolean)),
            Intrinsic::CharToInt => Some((TypeConstructor::Char, TypeConstructor::Int)),
            Intrinsic::IntToChar => Some((TypeConstructor::Int, TypeConstructor::Char)),
            _ => None,
        } {
            return Some(curried(vec![primitive(operand)], primitive(result)));
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
            | Intrinsic::IntZshr => (TypeConstructor::Int, TypeConstructor::Int),
            Intrinsic::I32Eq
            | Intrinsic::I32Ne
            | Intrinsic::I32LtS
            | Intrinsic::I32LeS
            | Intrinsic::I32GtS
            | Intrinsic::I32GeS => (TypeConstructor::Int, TypeConstructor::Boolean),
            Intrinsic::NumberAdd
            | Intrinsic::NumberSub
            | Intrinsic::NumberMul
            | Intrinsic::NumberDiv => (TypeConstructor::Number, TypeConstructor::Number),
            Intrinsic::NumberEq
            | Intrinsic::NumberNe
            | Intrinsic::NumberLt
            | Intrinsic::NumberLe
            | Intrinsic::NumberGt
            | Intrinsic::NumberGe => (TypeConstructor::Number, TypeConstructor::Boolean),
            Intrinsic::BooleanAnd | Intrinsic::BooleanOr => {
                (TypeConstructor::Boolean, TypeConstructor::Boolean)
            }
            Intrinsic::BooleanEq | Intrinsic::BooleanNe => {
                (TypeConstructor::Boolean, TypeConstructor::Boolean)
            }
            Intrinsic::CharEq
            | Intrinsic::CharNe
            | Intrinsic::CharLt
            | Intrinsic::CharLe
            | Intrinsic::CharGt
            | Intrinsic::CharGe => (TypeConstructor::Char, TypeConstructor::Boolean),
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
            | Intrinsic::IntToChar
            | Intrinsic::StringToBytes
            | Intrinsic::BytesToString
            | Intrinsic::Coerce
            | Intrinsic::Undefined => return None,
        };
        Some(curried(
            vec![primitive(operand), primitive(operand)],
            primitive(result),
        ))
    }
}

fn primitive(constructor: TypeConstructor) -> InferType {
    InferType::Constructor(constructor)
}

fn curried(arguments: Vec<InferType>, result: InferType) -> InferType {
    arguments
        .into_iter()
        .rev()
        .fold(result, |result, argument| arrow(argument, result))
}
