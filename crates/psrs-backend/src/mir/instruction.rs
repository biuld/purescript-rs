use crate::types::{HeapType, RefType, ValueId};
use psrs_core::Primitive;
use psrs_hir::SymbolId;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Constant {
        destination: ValueId,
        value: i32,
        span: TextRange,
    },
    StringConstant {
        destination: ValueId,
        bytes: String,
        span: TextRange,
    },
    Copy {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    Primitive {
        destination: ValueId,
        op: Primitive,
        left: ValueId,
        right: ValueId,
        span: TextRange,
    },
    Call {
        destination: ValueId,
        function: SymbolId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    /// A call to a runtime import that returns nothing.
    CallVoid {
        function: SymbolId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    RefNull {
        destination: ValueId,
        heap: HeapType,
        span: TextRange,
    },
    RefIsNull {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    RefTest {
        destination: ValueId,
        value: ValueId,
        reference: RefType,
        span: TextRange,
    },
    RefCast {
        destination: ValueId,
        value: ValueId,
        reference: RefType,
        span: TextRange,
    },
    I31New {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    I31GetS {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    StructNew {
        destination: ValueId,
        type_index: u32,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    StructGet {
        destination: ValueId,
        type_index: u32,
        field: u32,
        value: ValueId,
        span: TextRange,
    },
    StructSet {
        type_index: u32,
        field: u32,
        value: ValueId,
        new_value: ValueId,
        span: TextRange,
    },
    ArrayNew {
        destination: ValueId,
        type_index: u32,
        elements: Vec<ValueId>,
        span: TextRange,
    },
    ArrayGet {
        destination: ValueId,
        type_index: u32,
        value: ValueId,
        index: ValueId,
        span: TextRange,
    },
    ArraySet {
        type_index: u32,
        value: ValueId,
        index: ValueId,
        new_value: ValueId,
        span: TextRange,
    },
    ArrayLen {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    /// `i32.load`, for the canonical ABI and the byte-oriented WASI boundary.
    Load {
        destination: ValueId,
        address: ValueId,
        offset: u32,
        span: TextRange,
    },
    /// `i32.store`.
    Store {
        address: ValueId,
        value: ValueId,
        offset: u32,
        span: TextRange,
    },
    /// `i32.wrap_i64`, used to narrow a 64-bit WASI result to `Int`.
    WrapI64 {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
}

impl Instruction {
    /// The value this instruction defines, if it defines one.
    pub fn destination(&self) -> Option<ValueId> {
        match self {
            Self::Constant { destination, .. }
            | Self::StringConstant { destination, .. }
            | Self::Copy { destination, .. }
            | Self::Primitive { destination, .. }
            | Self::Call { destination, .. }
            | Self::RefNull { destination, .. }
            | Self::RefIsNull { destination, .. }
            | Self::RefTest { destination, .. }
            | Self::RefCast { destination, .. }
            | Self::I31New { destination, .. }
            | Self::I31GetS { destination, .. }
            | Self::StructNew { destination, .. }
            | Self::StructGet { destination, .. }
            | Self::ArrayNew { destination, .. }
            | Self::ArrayGet { destination, .. }
            | Self::ArrayLen { destination, .. }
            | Self::Load { destination, .. }
            | Self::WrapI64 { destination, .. } => Some(*destination),
            Self::StructSet { .. }
            | Self::ArraySet { .. }
            | Self::Store { .. }
            | Self::CallVoid { .. } => None,
        }
    }

    /// The values this instruction reads.
    pub fn operands(&self) -> Vec<ValueId> {
        match self {
            Self::Constant { .. } | Self::StringConstant { .. } => Vec::new(),
            Self::Copy { value, .. } => vec![*value],
            Self::Primitive { left, right, .. } => vec![*left, *right],
            Self::Call { arguments, .. } => arguments.clone(),
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
            Self::StructSet {
                value, new_value, ..
            } => vec![*value, *new_value],
            Self::ArrayGet { value, index, .. } => vec![*value, *index],
            Self::ArraySet {
                value,
                index,
                new_value,
                ..
            } => vec![*value, *index, *new_value],
            Self::Load { address, .. } => vec![*address],
            Self::Store { address, value, .. } => vec![*address, *value],
            Self::WrapI64 { value, .. } => vec![*value],
        }
    }

    pub fn span(&self) -> TextRange {
        match self {
            Self::Constant { span, .. }
            | Self::StringConstant { span, .. }
            | Self::Copy { span, .. }
            | Self::Primitive { span, .. }
            | Self::Call { span, .. }
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
            | Self::ArrayGet { span, .. }
            | Self::ArraySet { span, .. }
            | Self::ArrayLen { span, .. }
            | Self::Load { span, .. }
            | Self::Store { span, .. }
            | Self::WrapI64 { span, .. } => *span,
        }
    }
}
