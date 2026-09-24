use super::NumericOp;
use crate::types::{DefinedTypeId, HeapType, MemoryId, RefType, ValueId};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Copy {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    Constant {
        destination: ValueId,
        value: i32,
        span: TextRange,
    },
    NumberConstant {
        destination: ValueId,
        value: String,
        span: TextRange,
    },
    StringConstant {
        destination: ValueId,
        bytes: String,
        span: TextRange,
    },
    Primitive {
        destination: ValueId,
        op: NumericOp,
        left: ValueId,
        right: ValueId,
        span: TextRange,
    },
    UnaryPrimitive {
        destination: ValueId,
        op: super::UnaryOp,
        value: ValueId,
        span: TextRange,
    },
    Call {
        destination: ValueId,
        function: SymbolId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    RefFunc {
        destination: ValueId,
        function: SymbolId,
        type_index: DefinedTypeId,
        span: TextRange,
    },
    ClosureNew {
        destination: ValueId,
        function: SymbolId,
        type_index: DefinedTypeId,
        closure_type: DefinedTypeId,
        capture_array_type: DefinedTypeId,
        boxed_integer_type: Option<DefinedTypeId>,
        boxed_f64_type: Option<DefinedTypeId>,
        captures: Vec<ValueId>,
        span: TextRange,
    },
    CallRef {
        destination: ValueId,
        function: ValueId,
        type_index: DefinedTypeId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    ClosureCall {
        destination: ValueId,
        function: ValueId,
        type_index: DefinedTypeId,
        closure_type: DefinedTypeId,
        capture_array_type: DefinedTypeId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    ClosureGetCapture {
        destination: ValueId,
        closure: ValueId,
        closure_type: DefinedTypeId,
        capture_array_type: DefinedTypeId,
        boxed_integer_type: Option<DefinedTypeId>,
        boxed_f64_type: Option<DefinedTypeId>,
        index: u32,
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
        type_index: DefinedTypeId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    StructGet {
        destination: ValueId,
        type_index: DefinedTypeId,
        field: u32,
        value: ValueId,
        span: TextRange,
    },
    StructSet {
        type_index: DefinedTypeId,
        field: u32,
        value: ValueId,
        new_value: ValueId,
        span: TextRange,
    },
    ArrayNew {
        destination: ValueId,
        type_index: DefinedTypeId,
        elements: Vec<ValueId>,
        span: TextRange,
    },
    ArrayGet {
        destination: ValueId,
        type_index: DefinedTypeId,
        value: ValueId,
        index: ValueId,
        span: TextRange,
    },
    ArrayClone {
        destination: ValueId,
        type_index: DefinedTypeId,
        value: ValueId,
        span: TextRange,
    },
    ArraySet {
        type_index: DefinedTypeId,
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
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// Zero-extending `i32.load8_u`, used for one-byte canonical ABI tags.
    Load8U {
        destination: ValueId,
        address: ValueId,
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// `i32.store`.
    Store {
        address: ValueId,
        value: ValueId,
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// `i32.wrap_i64`, used to narrow a 64-bit WASI result to `Int`.
    WrapI64 {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    /// `i64.extend_i32_s`/`i64.extend_i32_u`, used to widen an `Int` argument to
    /// a 64-bit WASI parameter.
    WidenI64 {
        destination: ValueId,
        value: ValueId,
        signed: bool,
        span: TextRange,
    },
    /// Trap when a canonical ABI status value is nonzero.
    TrapIf { condition: ValueId, span: TextRange },
}

impl Instruction {
    /// The value this instruction defines, if it defines one.
    pub fn destination(&self) -> Option<ValueId> {
        match self {
            Self::Copy { destination, .. }
            | Self::Constant { destination, .. }
            | Self::NumberConstant { destination, .. }
            | Self::StringConstant { destination, .. }
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
            | Self::ArrayGet { destination, .. }
            | Self::ArrayClone { destination, .. }
            | Self::ArrayLen { destination, .. }
            | Self::Load { destination, .. }
            | Self::Load8U { destination, .. }
            | Self::WrapI64 { destination, .. }
            | Self::WidenI64 { destination, .. } => Some(*destination),
            Self::StructSet { .. }
            | Self::ArraySet { .. }
            | Self::Store { .. }
            | Self::CallVoid { .. }
            | Self::TrapIf { .. } => None,
        }
    }

    /// The values this instruction reads.
    pub fn operands(&self) -> Vec<ValueId> {
        match self {
            Self::Copy { value, .. } => vec![*value],
            Self::Constant { .. } | Self::NumberConstant { .. } | Self::StringConstant { .. } => {
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
            Self::Store { address, value, .. } => vec![*address, *value],
            Self::WrapI64 { value, .. }
            | Self::WidenI64 { value, .. }
            | Self::TrapIf {
                condition: value, ..
            } => vec![*value],
        }
    }

    pub fn span(&self) -> TextRange {
        match self {
            Self::Copy { span, .. }
            | Self::Constant { span, .. }
            | Self::NumberConstant { span, .. }
            | Self::StringConstant { span, .. }
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
            | Self::ArrayGet { span, .. }
            | Self::ArrayClone { span, .. }
            | Self::ArraySet { span, .. }
            | Self::ArrayLen { span, .. }
            | Self::Load { span, .. }
            | Self::Load8U { span, .. }
            | Self::Store { span, .. }
            | Self::WrapI64 { span, .. }
            | Self::WidenI64 { span, .. }
            | Self::TrapIf { span, .. } => *span,
        }
    }
}
