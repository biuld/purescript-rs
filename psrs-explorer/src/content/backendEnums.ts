import type { RepresentationId } from './types';

export type EnumVariant = { name: string; purpose: string };
export type BackendEnum = { name: string; source: string; purpose: string; variants: EnumVariant[] };

const cc = 'crates/psrs-backend/src/cc/';
const mir = 'crates/psrs-backend/src/mir/';
const shared = 'crates/psrs-backend/src/types.rs';
const wasm = 'crates/psrs-backend/src/wasm/mod.rs';

// This is a guide to variants present in the source, not a list of planned IR operations.
export const backendEnums: Partial<Record<RepresentationId, BackendEnum[]>> = {
  cc: [
    { name: 'AssignmentKind', source: `${cc}mod.rs`, purpose: 'One ordered CC computation assigned to a value.', variants: [
      { name: 'Constant', purpose: 'Produce an integer or Boolean scalar.' },
      { name: 'NumberConstant', purpose: 'Produce a Number literal.' },
      { name: 'StringConstant', purpose: 'Produce a string literal.' },
      { name: 'Primitive', purpose: 'Apply a source-independent primitive to two values.' },
      { name: 'DirectCall', purpose: 'Call a known function by stable symbol.' },
      { name: 'FunctionRef', purpose: 'Create a function value with an explicit signature and captures.' },
      { name: 'IndirectCall', purpose: 'Call a function value through its abstract signature.' },
      { name: 'ClosureGetCapture', purpose: 'Read a captured value by logical slot.' },
      { name: 'RepresentationTest', purpose: 'Test whether an erased value has a required reference shape.' },
      { name: 'RepresentationCast', purpose: 'Adapt an erased value to a required reference shape.' },
      { name: 'ProductNew', purpose: 'Construct a product or record from logical fields.' },
      { name: 'ProductGet', purpose: 'Read a product or record field by logical index.' },
      { name: 'VariantNew', purpose: 'Construct a tagged sum case from its logical fields.' },
      { name: 'VariantTag', purpose: 'Read the case tag without choosing a target dispatch strategy.' },
      { name: 'VariantGet', purpose: 'Read one logical field of a selected case.' },
      { name: 'ArrayNew', purpose: 'Construct an array from ordered elements.' },
      { name: 'ArrayLen', purpose: 'Read an array length.' },
      { name: 'ArrayGet', purpose: 'Read an array element.' },
      { name: 'ArrayClone', purpose: 'Copy an array before an immutable update.' },
      { name: 'ArraySet', purpose: 'Write an element in an array value being built.' },
      { name: 'If', purpose: 'Keep a value-producing conditional structured until CFG lowering.' },
    ] },
    { name: 'Representation', source: `${cc}representation.rs`, purpose: 'Target-neutral requirements for compound runtime values.', variants: [
      { name: 'Box', purpose: 'Require a reference wrapper around one value shape.' },
      { name: 'Product', purpose: 'Require ordered fields without choosing a physical layout.' },
      { name: 'Variant', purpose: 'Describe all tagged cases of one sum type and their field shapes.' },
      { name: 'Array', purpose: 'Require repeated elements of one shape.' },
    ] },
    { name: 'ValueShape', source: `${cc}representation.rs`, purpose: 'Logical value kinds before target type selection.', variants: [
      { name: 'Integer', purpose: 'Require an integer value.' },
      { name: 'Boolean', purpose: 'Require a Boolean value.' },
      { name: 'Number', purpose: 'Require a floating-point Number value.' },
      { name: 'Reference', purpose: 'Require a nullable or non-null reference with a RefShape.' },
    ] },
    { name: 'RefShape', source: `${cc}representation.rs`, purpose: 'Abstract heap identities carried by CC references.', variants: [
      { name: 'Repr', purpose: 'Refer to a requirement in the representation table.' },
      { name: 'Aggregate', purpose: 'Refer to an aggregate without choosing its concrete constructor layout.' },
      { name: 'Erased', purpose: 'Carry a polymorphic value in erased form.' },
      { name: 'Closure', purpose: 'Refer to a callable closure by abstract signature.' },
    ] },
  ],
  mir: [
    { name: 'Instruction', source: `${mir}instruction.rs`, purpose: 'Concrete target operations inside MIR basic blocks.', variants: [
      { name: 'Copy', purpose: 'Give an SSA value a second typed name, including an erased linear reference adaptation.' },
      { name: 'Constant', purpose: 'Define an i32 scalar constant.' },
      { name: 'NumberConstant', purpose: 'Define a floating-point constant.' },
      { name: 'StringConstant', purpose: 'Refer to string bytes lowered into target data.' },
      { name: 'Primitive', purpose: 'Compute a typed scalar primitive.' },
      { name: 'Call', purpose: 'Call a known function or value-returning import.' },
      { name: 'RefFunc', purpose: 'Create a typed reference to a known function.' },
      { name: 'ClosureNew', purpose: 'Allocate a GC closure and store its captures.' },
      { name: 'CallRef', purpose: 'Call a typed function reference.' },
      { name: 'ClosureCall', purpose: 'Extract and call code from a GC closure.' },
      { name: 'ClosureGetCapture', purpose: 'Read a capture from a GC closure.' },
      { name: 'CallVoid', purpose: 'Call a runtime import without a result.' },
      { name: 'RefNull', purpose: 'Create a null reference of a concrete heap type.' },
      { name: 'RefIsNull', purpose: 'Test a reference for null.' },
      { name: 'RefTest', purpose: 'Check a reference against a concrete reference type.' },
      { name: 'RefCast', purpose: 'Cast a reference to a concrete reference type.' },
      { name: 'I31New', purpose: 'Box a small integer in a Wasm i31 reference.' },
      { name: 'I31GetS', purpose: 'Recover a signed integer from an i31 reference.' },
      { name: 'StructNew', purpose: 'Allocate a concrete GC struct.' },
      { name: 'StructGet', purpose: 'Read a concrete GC struct field.' },
      { name: 'StructSet', purpose: 'Write a mutable GC struct field.' },
      { name: 'ArrayNew', purpose: 'Allocate a concrete GC array.' },
      { name: 'ArrayGet', purpose: 'Read a GC array element.' },
      { name: 'ArrayClone', purpose: 'Copy a GC array for an immutable update.' },
      { name: 'ArraySet', purpose: 'Write a GC array element.' },
      { name: 'ArrayLen', purpose: 'Read a GC array length.' },
      { name: 'Load', purpose: 'Load an i32 from linear memory.' },
      { name: 'Load8U', purpose: 'Load a byte from linear memory and zero-extend it.' },
      { name: 'Store', purpose: 'Store an i32 in linear memory.' },
      { name: 'LinearAlloc', purpose: 'Allocate a fixed-size linear-memory payload.' },
      { name: 'LinearAllocDynamic', purpose: 'Allocate a runtime-sized linear-memory payload.' },
      { name: 'LinearMemoryCopy', purpose: 'Copy a byte range between linear-memory payloads.' },
      { name: 'LinearLoad', purpose: 'Load a typed scalar with an explicit memory, offset, and alignment.' },
      { name: 'LinearStore', purpose: 'Store a typed scalar with an explicit memory, offset, and alignment.' },
      { name: 'LinearClosureNew', purpose: 'Allocate a linear closure with a function-table slot and captures.' },
      { name: 'LinearClosureCall', purpose: 'Call a linear closure through the function table.' },
      { name: 'LinearClosureGetCapture', purpose: 'Read a capture from a linear closure environment.' },
      { name: 'WrapI64', purpose: 'Narrow an i64 runtime result to an i32 Int.' },
      { name: 'WidenI64', purpose: 'Extend an i32 Int for a 64-bit runtime parameter.' },
      { name: 'TrapIf', purpose: 'Trap when a canonical ABI status signals failure.' },
    ] },
    { name: 'Terminator', source: `${mir}mod.rs`, purpose: 'The explicit control-flow exit of a MIR basic block.', variants: [
      { name: 'Return', purpose: 'Return the function result.' },
      { name: 'Jump', purpose: 'Transfer control and arguments to another block.' },
      { name: 'Branch', purpose: 'Choose then or else blocks and identify their merge block.' },
    ] },
    { name: 'ValueType', source: shared, purpose: 'Concrete MIR value types; Boolean is represented as i32 in Wasm.', variants: [
      { name: 'I32', purpose: '32-bit integer value.' }, { name: 'Boolean', purpose: 'Logical Boolean carried as i32.' },
      { name: 'I64', purpose: '64-bit integer value.' }, { name: 'F32', purpose: '32-bit float value.' },
      { name: 'F64', purpose: '64-bit float value.' }, { name: 'Ref', purpose: 'Typed, optionally nullable Wasm reference.' },
    ] },
    { name: 'HeapType', source: shared, purpose: 'The heap domain of a concrete reference type.', variants: [
      { name: 'Func', purpose: 'Any function reference.' }, { name: 'Extern', purpose: 'Host-managed reference.' },
      { name: 'Any', purpose: 'General GC reference.' }, { name: 'Eq', purpose: 'Equality-comparable GC reference.' },
      { name: 'I31', purpose: 'Tagged small integer reference.' }, { name: 'Struct', purpose: 'Any GC struct reference.' },
      { name: 'Array', purpose: 'Any GC array reference.' }, { name: 'Index', purpose: 'Reference to a module-defined concrete type.' },
    ] },
    { name: 'StorageType', source: shared, purpose: 'Concrete storage types of struct and array fields.', variants: [
      { name: 'I8', purpose: 'Packed 8-bit integer field.' }, { name: 'I16', purpose: 'Packed 16-bit integer field.' },
      { name: 'I32', purpose: '32-bit integer field.' }, { name: 'I64', purpose: '64-bit integer field.' },
      { name: 'F32', purpose: '32-bit float field.' }, { name: 'F64', purpose: '64-bit float field.' },
      { name: 'V128', purpose: '128-bit vector field.' }, { name: 'Ref', purpose: 'Reference field.' },
    ] },
    { name: 'CompositeType', source: shared, purpose: 'Concrete module-defined type bodies.', variants: [
      { name: 'Func', purpose: 'Function parameter and result types.' },
      { name: 'Struct', purpose: 'Ordered GC struct fields.' },
      { name: 'Array', purpose: 'GC array element field.' },
    ] },
  ],
  wasm: [
    { name: 'Op', source: wasm, purpose: 'Structured control flow; leaf instructions come from wasm-encoder.', variants: [
      { name: 'Leaf', purpose: 'Emit one Wasm instruction without duplicating its opcode enum.' },
      { name: 'If', purpose: 'Emit a structured then/else region with an optional result type.' },
    ] },
    { name: 'ExportKind', source: wasm, purpose: 'The kinds of entities this module exports.', variants: [
      { name: 'Function', purpose: 'Export a function.' }, { name: 'Memory', purpose: 'Export a linear memory.' },
    ] },
    { name: 'ExportIndex', source: wasm, purpose: 'A typed final index paired with an export kind.', variants: [
      { name: 'Function', purpose: 'Address an exported function.' }, { name: 'Memory', purpose: 'Address an exported memory.' },
    ] },
  ],
};
