use super::BlockId;
use super::NumericOp;
use crate::types::{DataId, DefinedTypeId, HeapType, MemoryId, RefType, ValueId};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

/// Which way a [`Instruction::ListCopy`] moves elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListDirection {
    /// Copy the GC array into the canonical buffer.
    Store,
    /// Allocate a GC array and fill it from the canonical buffer.
    Load,
    /// Free string payloads stored in a `list<string>` parameter buffer.
    FreeStrings,
}

/// One field of a `list<record>` element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListFieldCopy {
    /// A scalar field: a canonical slot at `offset`, GC field `index`.
    Scalar {
        offset: u32,
        index: u32,
        kind: crate::abi::layout::SlotKind,
    },
    /// A string field: canonical `(pointer, length)` at `offset`, GC string
    /// field `index`.
    String { offset: u32, index: u32 },
}

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
    /// Materializes a static string literal into a fresh GC string from a
    /// passive data segment holding its UTF-16 code units little-endian.
    ArrayNewData {
        destination: ValueId,
        type_index: DefinedTypeId,
        data_index: DataId,
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
    ArrayNewDefault {
        destination: ValueId,
        type_index: DefinedTypeId,
        length: ValueId,
        source: ValueId,
        header: BlockId,
        body: BlockId,
        exit: BlockId,
        index: ValueId,
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
    /// `i32.store8`, used for canonical booleans and narrow discriminants.
    Store8 {
        address: ValueId,
        value: ValueId,
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// `i32.store16`, used for canonical narrow discriminants.
    Store16 {
        address: ValueId,
        value: ValueId,
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// `i64.store`, used by indirect canonical parameters.
    StoreI64 {
        address: ValueId,
        value: ValueId,
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// `f32.store`, used by indirect canonical parameters.
    StoreF32 {
        address: ValueId,
        value: ValueId,
        memory: MemoryId,
        offset: u32,
        span: TextRange,
    },
    /// `f64.store`, used by indirect canonical parameters.
    StoreF64 {
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
    /// Element-wise copy between a GC array and a canonical `list<T>` buffer.
    /// P10 emits the loop; the extent checker only checks allocator provenance.
    ListCopy {
        direction: ListDirection,
        array: ValueId,
        array_type: DefinedTypeId,
        pointer: ValueId,
        length: ValueId,
        element: crate::abi::ListElement,
        span: TextRange,
    },
    /// Element-wise copy between a GC array of records and a canonical
    /// `list<record>` buffer. Each element is a directly flattened record of
    /// scalar fields; P10 emits a loop of struct gets/loads and stores.
    ListCopyRecord {
        direction: ListDirection,
        array: ValueId,
        array_type: DefinedTypeId,
        struct_type: DefinedTypeId,
        pointer: ValueId,
        length: ValueId,
        size: u32,
        fields: Vec<ListFieldCopy>,
        span: TextRange,
    },
    /// Trap when a canonical ABI status value is nonzero.
    TrapIf { condition: ValueId, span: TextRange },
    /// Produce a typed result on an unreachable path; Wasm's stack-polymorphic
    /// `unreachable` instruction supplies the result type to the verifier.
    Unreachable {
        destination: ValueId,
        span: TextRange,
    },
}

mod methods;
