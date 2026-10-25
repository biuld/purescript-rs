//! Target-neutral CC computations and structured branches.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub destination: ValueId,
    pub kind: AssignmentKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentKind {
    /// Retains the checked source operation at its explicit P8 erasure boundary.
    /// The runtime result is the existing value; Wasm encoding erases this witness.
    ControlEvidence {
        value: ValueId,
        implementation: ValueId,
        arguments: Vec<ValueId>,
        contract: ControlContractId,
    },
    Constant(i32),
    NumberConstant(String),
    StringConstant(String),
    Primitive {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    Unary {
        op: UnaryOp,
        value: ValueId,
    },
    Record {
        destination: ValueId,
        record: ValueId,
        operation: RecordOperation,
    },
    /// A source-checked `psrs:record-storage` binding projected into the
    /// application's shared record owner. Provider identity stays attached
    /// until P9 validates and realizes the operation.
    RecordPrimitive {
        provider: SymbolId,
        record: Option<ValueId>,
        operation: RecordOperation,
    },
    DirectCall {
        /// Required for predeclared machine helpers; absent for ordinary calls.
        contract: Option<ControlHelperId>,
        function: SymbolId,
        arguments: Vec<ValueId>,
    },
    FunctionRef {
        function: SymbolId,
        signature: SignatureId,
        captures: Vec<ValueId>,
    },
    IndirectCall {
        function: ValueId,
        signature: SignatureId,
        arguments: Vec<ValueId>,
    },
    /// A checked source runner supplies and discharges one logical root.
    ClosureGetCapture {
        closure: ValueId,
        index: u32,
    },
    RepresentationTest {
        destination: ValueId,
        value: ValueId,
        reference: Reference,
    },
    RepresentationCast {
        destination: ValueId,
        value: ValueId,
        reference: Reference,
    },
    AggregateConvert {
        destination: ValueId,
        value: ValueId,
        conversion: AggregateConvert,
    },
    ProductNew {
        destination: ValueId,
        representation: ReprId,
        arguments: Vec<ValueId>,
    },
    ProductGet {
        destination: ValueId,
        representation: ReprId,
        field: u32,
        value: ValueId,
    },
    VariantNew {
        destination: ValueId,
        representation: ReprId,
        case: u32,
        fields: Vec<ValueId>,
    },
    VariantTag {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    VariantGet {
        destination: ValueId,
        representation: ReprId,
        case: u32,
        field: u32,
        value: ValueId,
    },
    ArrayNew {
        destination: ValueId,
        representation: ReprId,
        elements: Vec<ValueId>,
    },
    ArrayFill {
        destination: ValueId,
        representation: ReprId,
        length: ValueId,
        value: ValueId,
    },
    ArrayLen {
        destination: ValueId,
        value: ValueId,
    },
    /// `Array.append`: a fresh array holding `left`'s elements followed by
    /// `right`'s. Both operands and the result name the same array
    /// representation, and neither operand is mutated.
    ArrayAppend {
        destination: ValueId,
        representation: ReprId,
        left: ValueId,
        right: ValueId,
    },
    /// A source `String`'s canonical UTF-8 bytes as an `Array Int`. A source
    /// string is a sequence of Unicode scalar values, so this is lossless.
    StringToBytes {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    /// Artifact-backed operation with checked language arguments before ABI erasure.
    RuntimeCall {
        intrinsic: psrs_hir::Intrinsic,
        arguments: Vec<ValueId>,
    },
    /// An `Array Int` read as a source `String`. Every element must be a
    /// canonical byte and the bytes must be well-formed UTF-8.
    BytesToString {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    ArrayGet {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        index: ValueId,
    },
    ArrayClone {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    ArraySet {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        index: ValueId,
        new_value: ValueId,
    },
    If {
        condition: ValueId,
        then_assignments: Vec<Assignment>,
        then_value: ValueId,
        else_assignments: Vec<Assignment>,
        else_value: ValueId,
    },
    /// A multi-way choice on a closed, integer-tagged data type. Pattern
    /// matching chooses these tags; P9 carries the choice into MIR's Switch.
    TagSwitch {
        value: ValueId,
        cases: Vec<TagCase>,
        default_assignments: Vec<Assignment>,
        default_value: ValueId,
    },
    /// Generic native protection. The exception binder is scoped to handler.
    Protect {
        body: Region,
        exception: ValueId,
        handler: Region,
    },
    /// Raise the native source-error channel with a String payload. This is
    /// bottom: its declared destination shape has no normal produced value.
    RaiseSourceError {
        message: ValueId,
    },
    /// An explicit trap result used by impossible decision-DAG edges.
    Unreachable,
}

/// One selected arm of a tag switch, with its branch-local computations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagCase {
    pub tag: i32,
    pub assignments: Vec<Assignment>,
    pub value: ValueId,
}

/// A structured region with an explicit normal or exceptional exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub assignments: Vec<Assignment>,
    pub exit: RegionExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionExit {
    Yield(ValueId),
    Propagate(ValueId),
}
