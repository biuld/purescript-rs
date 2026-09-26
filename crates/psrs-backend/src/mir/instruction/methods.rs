use super::{Instruction, ListDirection};
use crate::types::ValueId;
use psrs_span::TextRange;

impl Instruction {
    /// The value this instruction defines, if it defines one.
    pub fn destination(&self) -> Option<ValueId> {
        match self {
            Self::Copy { destination, .. }
            | Self::Constant { destination, .. }
            | Self::NumberConstant { destination, .. }
            | Self::ArrayNewData { destination, .. }
            | Self::Primitive { destination, .. }
            | Self::UnaryPrimitive { destination, .. }
            | Self::Call { destination, .. }
            | Self::RefFunc { destination, .. }
            | Self::ClosureNew { destination, .. }
            | Self::CallRef { destination, .. }
            | Self::ClosureCall { destination, .. }
            | Self::ClosureGetCapture { destination, .. }
            | Self::RefNull { destination, .. }
            | Self::RefIsNull { destination, .. }
            | Self::RefTest { destination, .. }
            | Self::RefCast { destination, .. }
            | Self::I31New { destination, .. }
            | Self::I31GetS { destination, .. }
            | Self::StructNew { destination, .. }
            | Self::StructGet { destination, .. }
            | Self::ArrayNew { destination, .. }
            | Self::ArrayNewDefault { destination, .. }
            | Self::ArrayGet { destination, .. }
            | Self::ArrayClone { destination, .. }
            | Self::ArrayLen { destination, .. }
            | Self::Load { destination, .. }
            | Self::Load8U { destination, .. }
            | Self::WrapI64 { destination, .. }
            | Self::WidenI64 { destination, .. }
            | Self::Unreachable { destination, .. } => Some(*destination),
            Self::ListCopy {
                direction: ListDirection::Load,
                array,
                ..
            } => Some(*array),
            Self::ListCopyRecord {
                direction: ListDirection::Load,
                array,
                ..
            } => Some(*array),
            Self::ListCopyFlags {
                direction: ListDirection::Load,
                array,
                ..
            } => Some(*array),
            Self::StructSet { .. }
            | Self::ArraySet { .. }
            | Self::Store { .. }
            | Self::Store8 { .. }
            | Self::Store16 { .. }
            | Self::StoreI64 { .. }
            | Self::StoreF32 { .. }
            | Self::StoreF64 { .. }
            | Self::CallVoid { .. }
            | Self::TrapIf { .. }
            | Self::ListCopy {
                direction: ListDirection::Store | ListDirection::FreeStrings,
                ..
            }
            | Self::ListCopyRecord {
                direction: ListDirection::Store | ListDirection::FreeStrings,
                ..
            }
            | Self::ListCopyFlags {
                direction: ListDirection::Store | ListDirection::FreeStrings,
                ..
            } => None,
        }
    }

    /// The values this instruction reads.
    pub fn operands(&self) -> Vec<ValueId> {
        match self {
            Self::Copy { value, .. } => vec![*value],
            Self::Constant { .. } | Self::NumberConstant { .. } | Self::ArrayNewData { .. } => {
                Vec::new()
            }
            Self::Primitive { left, right, .. } => vec![*left, *right],
            Self::UnaryPrimitive { value, .. } => vec![*value],
            Self::Call { arguments, .. } => arguments.clone(),
            Self::RefFunc { .. } => Vec::new(),
            Self::ClosureNew { captures, .. } => captures.clone(),
            Self::CallRef {
                function,
                arguments,
                ..
            } => std::iter::once(*function)
                .chain(arguments.iter().copied())
                .collect(),
            Self::ClosureCall {
                function,
                arguments,
                ..
            } => std::iter::once(*function)
                .chain(arguments.iter().copied())
                .collect(),
            Self::ClosureGetCapture { closure, .. } => vec![*closure],
            Self::CallVoid { arguments, .. } => arguments.clone(),
            Self::RefNull { .. } => Vec::new(),
            Self::RefIsNull { value, .. }
            | Self::RefTest { value, .. }
            | Self::RefCast { value, .. }
            | Self::I31New { value, .. }
            | Self::I31GetS { value, .. }
            | Self::StructGet { value, .. }
            | Self::ArrayLen { value, .. } => vec![*value],
            Self::StructNew { arguments, .. }
            | Self::ArrayNew {
                elements: arguments,
                ..
            } => arguments.clone(),
            Self::ArrayNewDefault { length, source, .. } => vec![*length, *source],
            Self::StructSet {
                value, new_value, ..
            } => vec![*value, *new_value],
            Self::ArrayGet { value, index, .. } => vec![*value, *index],
            Self::ArrayClone { value, .. } => vec![*value],
            Self::ArraySet {
                value,
                index,
                new_value,
                ..
            } => vec![*value, *index, *new_value],
            Self::Load { address, .. } | Self::Load8U { address, .. } => vec![*address],
            Self::ListCopy {
                direction: ListDirection::Load | ListDirection::FreeStrings,
                pointer,
                length,
                ..
            } => vec![*pointer, *length],
            Self::ListCopyRecord {
                direction: ListDirection::Load | ListDirection::FreeStrings,
                pointer,
                length,
                ..
            } => vec![*pointer, *length],
            Self::ListCopyFlags {
                direction: ListDirection::Load | ListDirection::FreeStrings,
                pointer,
                length,
                ..
            } => vec![*pointer, *length],
            Self::ListCopy {
                direction: ListDirection::Store,
                array,
                pointer,
                length,
                ..
            } => vec![*array, *pointer, *length],
            Self::ListCopyRecord {
                direction: ListDirection::Store,
                array,
                pointer,
                length,
                ..
            } => vec![*array, *pointer, *length],
            Self::ListCopyFlags {
                direction: ListDirection::Store,
                array,
                pointer,
                length,
                ..
            } => vec![*array, *pointer, *length],
            Self::Store { address, value, .. }
            | Self::Store8 { address, value, .. }
            | Self::Store16 { address, value, .. }
            | Self::StoreI64 { address, value, .. }
            | Self::StoreF32 { address, value, .. }
            | Self::StoreF64 { address, value, .. } => vec![*address, *value],
            Self::WrapI64 { value, .. }
            | Self::WidenI64 { value, .. }
            | Self::TrapIf {
                condition: value, ..
            } => vec![*value],
            Self::Unreachable { .. } => Vec::new(),
        }
    }

    pub fn span(&self) -> TextRange {
        match self {
            Self::Copy { span, .. }
            | Self::Constant { span, .. }
            | Self::NumberConstant { span, .. }
            | Self::ArrayNewData { span, .. }
            | Self::Primitive { span, .. }
            | Self::UnaryPrimitive { span, .. }
            | Self::Call { span, .. }
            | Self::RefFunc { span, .. }
            | Self::ClosureNew { span, .. }
            | Self::CallRef { span, .. }
            | Self::ClosureCall { span, .. }
            | Self::ClosureGetCapture { span, .. }
            | Self::CallVoid { span, .. }
            | Self::RefNull { span, .. }
            | Self::RefIsNull { span, .. }
            | Self::RefTest { span, .. }
            | Self::RefCast { span, .. }
            | Self::I31New { span, .. }
            | Self::I31GetS { span, .. }
            | Self::StructNew { span, .. }
            | Self::StructGet { span, .. }
            | Self::StructSet { span, .. }
            | Self::ArrayNew { span, .. }
            | Self::ArrayNewDefault { span, .. }
            | Self::ArrayGet { span, .. }
            | Self::ArrayClone { span, .. }
            | Self::ArraySet { span, .. }
            | Self::ArrayLen { span, .. }
            | Self::Load { span, .. }
            | Self::Load8U { span, .. }
            | Self::Store { span, .. }
            | Self::Store8 { span, .. }
            | Self::Store16 { span, .. }
            | Self::StoreI64 { span, .. }
            | Self::StoreF32 { span, .. }
            | Self::StoreF64 { span, .. }
            | Self::WrapI64 { span, .. }
            | Self::WidenI64 { span, .. }
            | Self::TrapIf { span, .. }
            | Self::Unreachable { span, .. }
            | Self::ListCopy { span, .. }
            | Self::ListCopyRecord { span, .. }
            | Self::ListCopyFlags { span, .. } => *span,
        }
    }
}
