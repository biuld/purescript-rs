# MIR: SSA/CFG and Verification

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md) and [CC IR](cc-ir.md);
the WebAssembly type system (GC structs and arrays, typed function references)
and the basics of SSA form and dominators. Read
[IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** MIR is the backend's lowest long-lived representation: a
target-specific, language-independent SSA control-flow graph whose values and
types follow the Wasm type system. It fixes runtime representation, calling
conventions, and the canonical import signatures, and it is the last place a
representation decision can be made before the thin Wasm encoding.

## Scope

This document owns the MIR model, the P9 representation-planner contract, the
lowering from CC to MIR, and MIR verification. It does not own the structure of
the Wasm encoder (see [Wasm encoding](../wasm/encoding-and-structuring.md)) or the
ABI adaptation rules (see [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md)).
Control-flow lowering from MIR to structured Wasm and tail calls are specified in
[control flow and tail calls](control-flow-and-tail-calls.md); scalar semantics
in [scalars and primitives](scalars-and-primitives.md); concrete GC layouts in
[data representation](data-representation.md); erased values in
[polymorphism and erasure](polymorphism-and-erasure.md).

## Background

**SSA and CFGs.** A program is a set of basic blocks. A block is a straight-line
list of instructions ending in a terminator. In static single assignment form,
every value is defined exactly once; uses refer to definitions. Control merges
use *block parameters*: a jump carries arguments, and the target block's
parameters receive them. This is the standard way to make φ-nodes explicit.

**SSA is functional.** Kelsey (1995) and Appel (*SSA is Functional Programming*,
1998) showed that SSA and continuation-passing style are two views of the same
program: a basic block is a continuation, a block parameter is a continuation
parameter, and a jump is a tail call. MIR is therefore not a departure from the
functional pipeline; it is the continuation form of the CC program, which is why
representation can be fixed and verified here.

**Dominance.** A definition dominates a use when every path from the function
entry to the use passes through the definition. SSA validity requires that every
use is dominated by its definition. Dominators are computed by the standard
iterative fixed point over the predecessor relation.

**The Wasm type system.** Wasm GC gives nominal struct and array types, nullable
and non-nullable references, and a subtyping relation between defined types.
Typed function references allow a value of function type to be called with
`call_ref`. These are the types MIR values carry. Because PureScript sums must be
encoded in this nominal system, the design uses a tag-carrying abstract supertype
plus one final subtype per case (see [data representation](data-representation.md)).

**Structured targets.** Wasm control flow is structured (`block`/`loop`/`if`),
while MIR is an arbitrary graph. Recovering structure is a separate, later step;
MIR deliberately does not carry structure. See
[control flow and tail calls](control-flow-and-tail-calls.md).

## Model

### Values and types

```text
ValueType  = I32 | Boolean | I64 | F32 | F64 | Ref(RefType)
RefType    = { nullable: bool, heap: HeapType }
HeapType   = Func | Extern | Any | Eq | I31 | Struct | Array | Index(DefinedTypeId)

StorageType = Val(ValueType) | Ref(RefType)
FieldType   = { storage: StorageType, mutable: bool }
CompositeType = Func { parameters: [ValueType], results: [ValueType] }
              | Struct([FieldType])
              | Array(FieldType)
DefinedType = { final_type: bool, supertype: Option(DefinedTypeId), composite: CompositeType }
RecGroup    = [DefinedType]
```

`Boolean` is a logical type encoded as `i32`; the verifier treats it as `i32`
where a value flows, and a `Constant` that produces it must be `0` or `1`.

Identifiers are deliberately distinct types even though all encode as `u32`:
`DefinedTypeId`, `FunctionId`, `MemoryId`, `DataId`, `ValueId`, `BlockId`, and
the source-level `SymbolId`. Mixing index spaces is a bug the type system
prevents.

### Module, function, and block

```text
Module   = { name, types: [RecGroup], imports: [Import], functions: [Function],
             entry: Option(SymbolId), span }
Import   = { symbol: SymbolId, parameters: [ValueType], result: Option(ValueType) }
Function = { id: FunctionId, symbol: SymbolId, name,
             parameters: [ValueId], values: [ValueDecl], entry: BlockId,
             blocks: [BasicBlock], result: ValueId, result_type: ValueType, span }
BasicBlock = { id: BlockId, parameters: [ValueId], instructions: [Instruction],
               terminator: Option(Terminator) }
ValueDecl  = { id: ValueId, ty: ValueType }
```

`types` is the P9-owned concrete type table. It is emitted before the function
types and indexed by `DefinedTypeId`; P10 later computes the final Wasm indices.

### Terminators

```text
Terminator = Return    { value: ValueId }
           | Jump      { target: BlockId, arguments: [ValueId] }
           | Branch    { condition: ValueId, then_block: BlockId, else_block: BlockId }
           | Switch    { value: ValueId, cases: [(i32, BlockId)], default: BlockId }
           | ReturnCall     { function: SymbolId, arguments: [ValueId] }
           | ReturnCallRef  { function: ValueId, arguments: [ValueId] }
```

`Branch` omits `merge_block` in the target MIR model because the structurer
should discover joins from the graph
([control flow and tail calls](control-flow-and-tail-calls.md)). The current
MIR still carries `merge_block` and verifies its one-value merge contract; the
implementation status is recorded below. `Switch` dispatches on an `i32` tag
and is the target lowering for constructor matches. `ReturnCall`/`ReturnCallRef`
are tail calls in the target model.

### Instructions

```text
Copy, Constant, NumberConstant, StringConstant,
Primitive, UnaryPrimitive,
Call, CallVoid, RefFunc, ClosureNew, CallRef, ClosureCall, ClosureGetCapture,
RefNull, RefIsNull, RefTest, RefCast, I31New, I31GetS,
StructNew, StructGet, StructSet,
ArrayNew, ArrayGet, ArraySet, ArrayLen, ArrayClone,
Load, Load8U, Store, WrapI64, WidenI64, TrapIf
```

They group into: scalar constants, arithmetic, and conversions; direct and
closure calls; reference and GC construction and observation; arrays; and the
canonical ABI byte and width operations. Scalar operations are defined in
[scalars and primitives](scalars-and-primitives.md); closure operations in the
Design section below.

### Invariants

- Blocks are unique; the entry block exists; every block has a terminator.
- Values are declared once and defined once: by a function parameter, a block
  parameter, or exactly one instruction.
- Every use is dominated by its definition.
- Jump arguments match the target block's parameters in arity and type.
- Branch conditions are `Boolean`; return values equal `result_type`.
- `FunctionId`s are unique and equal to the function's module position.
- Import symbols are unique.
- Types only reference defined types in range, and subtyping links are valid.

## Design

### Representation planning

P9 is the only stage that may choose runtime representation. It runs a
*representation planner* over the reachable CC requirements and produces a
`PlannedLayout`: a mapping from `ReprId`/`SignatureId` to `DefinedTypeId`, plus
the `RecGroup`s that become MIR's type table.

- **Reachability.** Only requirements reachable from the CC module become types.
  Dead boxes, variants, or closures do not appear. The worklist walks value
  shapes, operations, external signatures, and referenced handles.
- **Ordering.** A defined type must precede its uses; in particular a variant
  supertype precedes its case subtypes. P9 assigns `DefinedTypeId`s in this
  order and puts every definition in a single recursion group, which admits
  recursive and mutually recursive references within that group. IDs are
  reserved before definitions are filled; forward references inside the group
  are valid. Only a supertype edge must point to a previously declared type.
- **GC target.** The planner uses `struct`, `array`, typed function references,
  `ref.test`, `ref.cast`, and `call_ref`. A profile without GC or function
  references is rejected here with a source-associated diagnostic, never by a
  silent fallback.

The planner contract is a trait with a single implementation today; the point of
the trait is that CC never mentions a concrete layout.

### Closure representation

A function value is a GC struct with two fields: a typed function reference to
the lifted code, and a capture array. Captures are stored in one uniform array of
nullable `eqref` so the closure type does not depend on the capture types:

- `Boolean` captures are boxed as `i31`;
- `Int` and `String` captures are boxed in a one-field struct so all 32-bit
  values and addresses round-trip;
- `f64` captures are boxed in a one-field struct;
- reference captures are stored as they are;
- erased captures are already `eqref`.

Creation is `ClosureNew` (`ref.func` + boxing + `array.new_fixed` + `struct.new`);
a closure call extracts the code reference and uses `call_ref`; capture
projection reads the array and unboxes. The concrete layouts are in
[data representation](data-representation.md).

### Sum and product representation

A product is a GC struct whose fields are the product's elements. A sum is an
abstract, non-final struct carrying the tag plus one final struct per case, per
[CC IR](cc-ir.md); a
sum whose cases are all nullary is an immediate `i32` tag and allocates nothing.
Arrays are GC arrays with a mutable element type. Strings are `i32` pointers to
length-prefixed UTF-8 buffers at the canonical ABI boundary
([linear memory boundary](../wasm/linear-memory-and-canonical-abi-boundary.md)).

### Imports and the ABI boundary

MIR imports are canonical ABI signatures: a symbol, parameter `ValueType`s, and
an optional result type. The WIT names, interfaces, and worlds stay in the ABI
registry beside MIR; MIR records only the symbols a lowered call references. P9
inserts the adaptation instructions (address constants, loads/stores, i64
width conversions, return-area handling) so the ABI layer never leaks into CC.

### Rejected alternatives

- **A structured result IR instead of a CFG.** Rejected: SSA/CFG is the natural
  substrate for optimization and its equivalence to continuations is well
  understood; structure is recovered once, late, in the Wasm structurer.
- **Keeping expression trees into MIR.** Rejected: evaluation order and
  temporaries would stay implicit, and dominance-based verification would be
  impossible.
- **Target-neutral MIR.** Rejected: representation must be fixed somewhere before
  encoding, and choosing it at the Wasm boundary conflates structuring with
  representation; MIR is explicitly target-specific.
- **`merge_block` hints as the permanent structuring contract.** Rejected as a
  long-term design: general structuring supersedes it
  ([control flow](control-flow-and-tail-calls.md)).

## Algorithms

### Reachability

```text
work = requirements of every value, result, and assignment of the CC module
seen = {}
while work is non-empty:
    r = pop(work)
    if r in seen: continue
    seen.add(r)
    for each value shape referenced by r:
        if it is a Repr/Closure not in seen: push it
collect externals of referenced direct calls
```

### Planning

```text
plan_selected(table, reachable):
    assign a DefinedTypeId to each reachable ReprId, in sorted order
    for each variant requirement:
        its supertype gets the id of the requirement itself
        each case gets a fresh id with that supertype
    for each closure signature:
        allocate the capture array type and the closure struct type
    build one RecGroup from the definitions, supertypes before subtypes;
    validate all references after the group is complete
```

### Lowering CC to SSA

Each CC `Assignment` becomes an instruction defining its destination `ValueId`,
in the order CC fixed (so evaluation order is preserved). Structured CC control
becomes a CFG:

```text
lower_if(cond, then_assignments, then_value, else_assignments, else_value):
    then = fresh block; else = fresh block; merge = fresh block
    merge.parameters = [destination]
    terminate current with Branch(cond, then, else)
    lower then_assignments into then; terminate then with Jump(merge, [then_value])
    lower else_assignments into else; terminate else with Jump(merge, [else_value])
    current = merge
```

A direct call becomes `Call`/`CallVoid`; a closure call becomes `ClosureCall`.
The scalar helpers of `scalar_helpers` are appended as extra functions when the
module uses floor division or modulo.

### Dominance

```text
for each block b: dom(b) = all blocks
dom(entry) = { entry }
repeat until fixed point:
    dom(b) = { b } ∪ intersection(dom(p) for p in predecessors(b))
a definition in block d dominates a use in block u iff d ∈ dom(u) and d appears
before the use within a block
```

### Verification entry points

`verify_module` checks structural invariants; `verify_module_with_capabilities`
adds target legality. P10 re-runs the same verifier before encoding, so no
unverified MIR reaches the encoder.

## Code map

The `mir/` module owns the MIR model, the P9 lowering, and MIR verification. It
is the only module that may use the Wasm value/reference type model or fix a
runtime representation; P10 consumes the type table it produces and MUST NOT
choose, deduplicate, or reorder a representation. See
[IR boundaries](../00-ir-boundaries.md) for the crate-level tree. The intended
structure is:

```text
mir/
  mod.rs           Module, Function, BasicBlock, Terminator, Import,
                   ValueId, BlockId, DefinedTypeId, ValueType, RecGroup;
                   the P9 entry points
  instruction.rs   Instruction and destination/operands/span
  planner.rs       RepresentationPlanner trait and GcPlanner
  layout/          PlannedLayout and concrete GC layouts
  lower/           CC -> MIR lowering
  verify/          MIR verifier
  wit/             canonical ABI adaptation for WIT calls
  reachable.rs     reachability of representation requirements
  scalar_helpers.rs, numeric.rs   scalar operations and helpers
```

**Required types.** `mir/mod.rs` MUST define `Module`, `Function`,
`BasicBlock`, `Terminator`, `Import`, and the identifiers `ValueId`, `BlockId`,
`DefinedTypeId`, `ValueType`, and `RecGroup`, with the shapes of the
[Model](#model):

```rust
pub struct Module { name, types: Vec<RecGroup>, imports: Vec<Import>, functions: Vec<Function>, entry: Option<SymbolId>, span }
pub struct Import { symbol: SymbolId, parameters: Vec<ValueType>, result: Option<ValueType> }
pub struct Function { id: FunctionId, symbol: SymbolId, name, parameters: Vec<ValueId>, values: Vec<ValueDecl>, entry: BlockId, blocks: Vec<BasicBlock>, result: ValueId, result_type: ValueType, span }
pub struct BasicBlock { id: BlockId, parameters: Vec<ValueId>, instructions: Vec<Instruction>, terminator: Option<Terminator> }
pub enum Terminator { Return, Jump, Branch, Switch, ReturnCall, ReturnCallRef }
```

`mir/instruction.rs` MUST define `Instruction` so every variant carries a
destination `ValueId`, its operand `ValueId`s, and a source span. The scalar
vocabularies live in `mir/numeric.rs` and `mir/scalar_helpers.rs`
([scalars and primitives](scalars-and-primitives.md)); `mir/layout/` owns
`PlannedLayout` and every concrete GC layout
([data representation](data-representation.md)); `mir/wit/` owns canonical ABI
adaptation ([canonical ABI and WIT](../wasm/canonical-abi-and-wit.md)); and
`mir/reachable.rs` owns the reachability walk.

**Required entry points.** `mir/mod.rs` MUST expose exactly these entry points:

```rust
pub fn lower_module(module: cc::Module) -> Result<(Module, WasiRegistry), Vec<BackendError>>;
pub fn lower_module_with_capabilities(module: cc::Module, target: TargetCapabilities) -> Result<(Module, WasiRegistry), Vec<BackendError>>;
pub fn lower_module_with_bindings(module: cc::Module, bindings: ExternalBindings, target: TargetCapabilities) -> Result<(Module, WasiRegistry), Vec<BackendError>>;
pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>>;
pub fn verify_module_with_capabilities(module: &Module, target: TargetCapabilities) -> Result<(), Vec<BackendError>>;
```

Each `lower_*` function MUST validate the bindings against CC, plan the
reachable requirements, and return the module together with the ABI registry,
so MIR stores no WIT or component detail. Each `verify_*` function MUST check
the invariants of [Invariants and verification](#invariants-and-verification);
`verify_module_with_capabilities` additionally enforces legality under the
selected [capability profile](../wasm/capability-profile.md). P10 re-runs the
same verifier before encoding.

**Planner contract.** `mir/planner.rs` MUST define the planner trait and its
single GC implementation:

```rust
pub trait RepresentationPlanner {
    type Layout;
    fn plan_module(&self, module: &cc::Module) -> Result<Self::Layout, LayoutError>;
}
pub struct GcPlanner { pub target: TargetCapabilities }
impl RepresentationPlanner for GcPlanner { type Layout = PlannedLayout; }
```

No module above MIR may depend on the planner, its layout, or a concrete
`DefinedTypeId`.

**Lowering split.** `mir/lower/` MUST organize CC-to-MIR lowering by concern:
`mod.rs` drives module and function lowering and appends generated helpers;
`assignments.rs` lowers single-assignment operations and selects the scalar
instruction; variant and aggregate lowering lives beside them. Structured
control lowering is specified in
[control flow and tail calls](control-flow-and-tail-calls.md).

**Verifier split.** `mir/verify/` MUST split by concern: `mod.rs` exposes the
entry points and owns module-level type and signature setup; `function.rs`
checks SSA, dominance, blocks, and terminators; `instruction/` checks each
instruction class; `call.rs` checks direct, closure, and reference calls and
`ref.func` agreement; `subtype.rs` checks recursion-group and subtyping
well-formedness; `capability.rs` checks target legality.

## Invariants and verification

The MIR verifier checks:

- type and recursion-group well-formedness, including subtyping (a supertype is
  declared and non-final; struct fields form a width/depth-compatible prefix with
  invariant mutable fields; array elements are subtypes; function parameters are
  contravariant and results covariant);
- uniqueness of blocks, values, imports, functions, and concrete type IDs;
- exactly one definition per SSA value, and definition-before-use with dominance
  across blocks;
- block-parameter and jump-argument arity and exact types;
- branch conditions and return values, and `Switch` case uniqueness;
- constant and primitive operand/result types;
- exact direct, closure, and reference-call signatures and `ref.func` agreement;
- reference nullability, casts, aggregate fields, array elements, and closure
  operations against the concrete type table;
- canonical ABI memory operations: memory identity, `i32` address, and access
  type;
- canonical import calls against the MIR import table;
- legality of every type and instruction under the selected capability profile.

Failure is a compiler bug or an unsupported program; it is reported with the
operation's source span and the declaring module, never as invalid output.

## Worked example

Take a CC fragment for `main = let p = MkProduct 7 2.5 in field0 p`:

```text
v0 = 7
v1 = 2.5
v2 = ProductNew Product[Integer, Number](v0, v1)
v3 = ProductGet Product[Integer, Number](field 0, v2)
result = v3
```

Planning assigns the product one `Struct` with fields `[i32, f64]` and gives
`main` the function type `() -> i32`. The lowered MIR is a single block:

```text
block B0:
    v0 = Constant 7
    v1 = NumberConstant "2.5"
    v2 = StructNew $product(v0, v1)
    v3 = StructGet $product(field 0, v2)
    Return v3
```

If instead the source branches, `lower_if` produces three blocks: `B0` branches
to `B1`/`B2`, each computes its value and jumps to `B3` with one argument, and
`B3` returns its parameter. The verifier proves every use of `B3`'s parameter is
dominated by the block, since `B3`'s parameter is defined at its entry.

## Boundaries and interfaces

- **Input:** a verified CC module plus the external bindings (`ExternalBindings`)
  and an explicit `TargetCapabilities` profile.
- **Output:** a verified MIR module plus the ABI registry, which P10 uses to name
  imports.
- **To P10:** verified MIR, the concrete type table, and typed resource IDs; P10
  assigns final Wasm indices mechanically and must not choose a representation.
- **From the frontend:** the driver links and prunes Typed Core; P8 lowers it to
  CC. MIR never sees source syntax.

## Open questions and future work

- **Control flow.** MIR `Switch` is implemented for nullary-constructor matches
  with unique constructor tags; duplicate alternatives retain an ordered
  comparison chain. For acyclic functions, the Wasm structurer preserves the
  existing merge-based diamond and switch-join lowering. For cyclic CFGs, it
  computes dominators and natural loops, validates loop nesting, and emits
  `Loop` regions with continuation `Block`s and depth-relative branches; tests
  cover loop-carried values, nested loops, and multiple exits. Irreducible CFGs
  are diagnosed, and the dispatcher fallback remains future work. Tail-call
  marking and lowering to `ReturnCall`/`ReturnCallRef` also remain future work.
  Current MIR retains `Branch { merge_block }` and its one-value verifier
  contract.
- **Multi-value.** The type model admits multiple function results, but functions
  and calls currently have one. Tuples are represented as products in the
  meantime.
- **Optimization.** [MIR optimization](../opt/mir.md) specifies P10 passes.
  Unboxing across call boundaries remains a P9 representation decision.
- **Memory access extents.** The ABI boundary design defines static address,
  interval, and region-permission checks. The Wasm verifier implementation is
  underway; dynamic reads retain runtime bounds traps, while stores require a
  statically known writable ABI region.

## Implementation notes

P9 lowers verified CC to MIR with its concrete type table. The backend then
runs P10 MIR optimization before P11 Wasm lowering. P10 verifies MIR on entry
and after each pass. It inlines only internal direct calls to single-block
callees whose sole basic block has no block parameters, at most 16
instructions, a `Return` terminator, and no call-like instructions in the body
(direct, closure, or function-reference calls). This is a per-callee limit;
the implementation has no separate call-site-count or total code-growth
budget. Cloned instructions remain in order at the call site, preserving traps
and memory effects. P10 then iterates unreachable-block pruning, constant
propagation, and Branch/Switch simplification to a fixed point. It also
forwards copies, eliminates dead pure-and-total instructions using
conservative call, memory, and trap effects, and removes imports unused by
reachable optimized code.
P9 validates external declarations before import projection. Optimization
preserves the P9 type table and function signatures; planned types made unused
by optimization may remain.

Current MIR emits `Return`, `Jump`, `Branch { merge_block }`, and `Switch`;
nullary-constructor cases with unique tags lower to `Switch`, while duplicate
alternatives keep their source-order comparison chain. P10 preserves and can
simplify switches. Acyclic functions retain the merge-based diamond and
switch-join structurer. For cyclic MIR CFGs, P10 computes dominators and natural
loops, checks that loop regions are nested, and emits Wasm `Loop` regions with
continuation `Block`s and depth-relative branches. Loop fixtures use direct MIR
because CC-to-MIR does not yet produce loops. Irreducible CFGs are diagnosed;
the dispatcher fallback is not implemented. The current MIR and verifier still
retain `Branch { merge_block }` and its one-value merge contract; cyclic
structuring follows CFG edges, while the acyclic path uses the merge hint.
`ReturnCall` and `ReturnCallRef` are not in the current MIR terminator set;
tail-call marking and self-recursion loopification remain unimplemented.

## References

- Appel, *SSA is Functional Programming* (1998).
- Kelsey, *A Correspondence between Continuation Passing Style and Static Single
  Assignment Form* (1995).
- Flanagan, Sabry, Duba, and Felleisen, *The Essence of Compiling with
  Continuations* (1993).
- WebAssembly 3.0: GC, typed function references, and the core type system.
- [CC IR](cc-ir.md),
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).
