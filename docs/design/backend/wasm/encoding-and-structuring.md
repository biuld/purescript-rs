# Wasm Encoding and Structuring

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Stable (design)  
**Prerequisites:** [MIR](../fp/mir.md) and [control flow and tail calls](../fp/control-flow-and-tail-calls.md); the WebAssembly 3.0 binary format, its structured control instructions, and its separate type, function, memory, and data index spaces; the idea of recovering structured control flow from a CFG (the Relooper, the LLVM WebAssembly stackifier). Read [IR boundaries](../00-ir-boundaries.md) and [DEC-02](../../../decision/DEC-02-thin-structured-wasm-encoding.md) first.  
**Summary:** The thin structured Wasm encoding is the target representation that sits between MIR and the binary artifact. It models only the module skeleton and structured control regions; leaf opcodes are `wasm_encoder::Instruction` values, so the encoding does not re-declare the WebAssembly instruction set. P10 structures the CFG, maps MIR identities to final Wasm indices, and P11 encodes the sections and validates the result.

## Scope

This document owns the thin Wasm IR, the structuring of MIR control flow into
structured regions, final index allocation, the declared element segment for
`ref.func`, passive literal segments, the synthesized command entry, and binary
emission. It does not own:

- the MIR model or its CFG, which is specified in [MIR](../fp/mir.md);
- general control-flow structuring and tail calls, whose algorithms live in
  [control flow and tail calls](../fp/control-flow-and-tail-calls.md);
- concrete scalar, GC, and closure layouts, which MIR fixes;
- canonical ABI adaptation, which is [canonical ABI and WIT](canonical-abi-and-wit.md);
- target capability policy, which is the [capability profile](capability-profile.md);
- component packaging and WASI services, which is the
  [WASI platform library](wasi-platform-library.md).

## Background

**Structured control flow.** WebAssembly is not a graph of jumps. Its control
instructions are `block`, `loop`, and `if`, each introducing a region delimited
by `end` (and `else` for `if`). A `block` branches forward to its end, a `loop`
branches backward to its head, and an `if` selects one of two regions. Branch
instructions (`br`, `br_if`, `br_table`) name a target by its depth in the stack
of enclosing labels, not by an address. A region may consume operands and yield
results, so a value-producing `if` can return a value to its continuation. Any
program whose control flow is reducible can be expressed this way; irreducible
graphs need a dispatcher loop over a state local. The Relooper (Zakai) and the
LLVM WebAssembly stackifier (Gohman) are the standard recovery algorithms.

**CFG versus tree.** MIR is an arbitrary graph of basic blocks with block
parameters. Wasm requires a tree of nested regions. MIR therefore deliberately
carries no structure; recovering it is a distinct, late, independently
verifiable step, and [DEC-02](../../../decision/DEC-02-thin-structured-wasm-encoding.md)
rejects both of the alternatives that would blur it: a full Wasm IR that mirrors
the opcode set, and encoding bytes directly while structuring.

**Binary format and index spaces.** A core module is a sequence of sections in a
fixed order: type, import, function, table, memory, global, export, start,
element, code, data. Each entity kind has its own index space. Imported
functions occupy the start of the function index space, defined functions follow
in function-section order, and a function's body is found by subtracting the
import count. Types are defined types first (in the GC case, recursion groups),
then function types. `ref.func` may only reference a function that is *declared*
somewhere in the module, which an active or declared element segment provides.

**Data segments.** Passive data segments carry the string literals, which P10
materializes into GC strings with `array.new_data`; each distinct literal is
materialized once through a lazily initialized global. An active segment
initializes the allocator's heap-state region. Together they service the
canonical ABI boundary and are not a language object heap
([linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
[DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)).

## Model

The thin encoding is a small Rust data model. Raw `u32` indices are produced
only by `encode_module`; until then, values carry one of four distinct index
domains so an index cannot be used in the wrong space.

```text
Module   = { name: String,
             imports: [Import],
             types: [FuncType],
             type_defs: [RecGroup],
             functions: [Function],
             memories: [Memory],
             globals: [Global],
             data: [DataSegment],
             exports: [Export],
             entry: Option(Entry),
             realloc: Option(Function),
             span: TextRange }

Import   = { module: String, name: String, type_index: TypeIndex }
FuncType = { parameters: [ValType], results: [ValType] }

Function = { symbol: SymbolId, name: String, type_index: TypeIndex,
             parameters: [ValType], locals: [ValType],
             body: Body, span: TextRange }

Body = [Op]
Op   = Leaf(wasm_encoder::Instruction)
     | If    { then_body: Body, else_body: Body, result: Option(ValType),
               span: TextRange }
     | Block { body: Body, result: Option(ValType), span: TextRange }
     | Loop  { body: Body, result: Option(ValType), span: TextRange }

Entry        = { type_index: TypeIndex, body: Body }
Memory       = { id: MemoryId, index: MemoryIndex,
                 minimum: u64, maximum: Option(u64) }
Global       = { index: GlobalIndex, mutable: bool, ty: ValType,
                 init: RefNull(HeapType) }
DataSegment  = { id: DataId, index: DataIndex,
                 mode: Active(offset: u32) | Passive, bytes: [u8] }
Export       = { name: String, kind: ExportKind, index: ExportIndex }
ExportKind   = Function | Memory
ExportIndex  = Function(FunctionIndex) | Memory(MemoryIndex)
```

The final index domains are `TypeIndex`, `FunctionIndex`, `MemoryIndex`,
`GlobalIndex`, and `DataIndex`. They are distinct types, and distinct from MIR's
module-local `DefinedTypeId`, `FunctionId`, `MemoryId`, and `DataId`. `Entry`,
the optional `realloc` function, and the literal-interning globals are
synthesized by P10 and therefore have no MIR identity of their own;
`Module::defined_type_count` reports how many entries of the type index space
are defined (non-function) types.

### Invariants

- Defined types (`type_defs`) occupy the start of the type index space,
  flattened in recursion-group order; function types (`types`) follow at
  `defined_type_count()`. A `TypeIndex` below that bound must name a defined
  function type or a defined struct/array type as the referring construct
  requires.
- The function index space is imports, then `functions` in order, then the
  synthesized `entry`, then `realloc` when present. `FunctionIndex` values are
  consistent with that order.
- `MemoryId(0)` and `DataId`/`DataIndex` are assigned in vector order; data
  segment `index.0` equals its position.
- Global indices are assigned in vector order, with `index.0` equal to the
  global's position; a global's `RefNull` initializer names the same nullable
  reference type as its value type.
- Every `Op::If` is immediately preceded in its body by the instruction that
  pushes its condition.
- A `Leaf(Instruction::RefFunc(f))` index is in range and appears in the
  declared element segment.
- The `entry` function, when present, is zero-argument and returns one `i32`;
  when the WASI CLI capability is enabled its body calls `exit-with-code` with
  `main`'s result.

## Design

### One representation boundary

MIR already fixed every runtime layout and calling convention. P10 performs only
three jobs: recover structure, assign final indices mechanically, and build the
module skeleton. It must not create types, change a sum or closure encoding,
introduce ABI adaptation, or infer a missing layout
([IR boundaries](../00-ir-boundaries.md)). The thin IR therefore has a `Leaf`
that carries a raw `wasm_encoder::Instruction` plus the structured regions the
language's control shapes need: `If`, `Block`, and `Loop`. `wasm-encoder` owns
the opcode set; the encoding grows with the language's control shapes, not with
the WebAssembly instruction count.

### Structuring

The structurer consumes a MIR function and produces one `Body`. It has exactly
two paths, chosen by the CFG analysis in
[control flow and tail calls](../fp/control-flow-and-tail-calls.md):

- **Reducible.** Every reducible CFG, with or without loops, is reduced to one
  `RegionPlan`: reachable blocks are partitioned into nested regions, each a
  topologically ordered list of block and loop units. A single emitter walks
  that plan and emits each unit once inside nested `Op::Block`s, opens an
  `Op::Loop` at each natural-loop header, and resolves `Jump`, `Branch`, and
  `Switch` targets to depth-relative `br`/`br_if`/`br_table`. A join is not
  required for emission: a branch whose arms both return targets their labels
  directly.
- **Irreducible.** Only a cycle with more than one entry cannot be nested
  statically; it uses the dispatcher fallback over an `i32` state local.

Each MIR instruction is emitted as leaf opcodes with an explicit `LocalSet`
(MIR values are Wasm locals). Each terminator decides the control shape:
`Return` ends the region; `Jump` reads the jump arguments, writes the target
block's parameter locals, and branches to the target unit's label; `Branch` and
`Switch` load their selector and branch to the selected unit's label. MIR
carries no structuring hint, and `common_join` is an analysis aid, not an
emission precondition. Structuring is the only place that knows about Wasm
region shape; MIR stays structure-free.

### Index allocation

Index assignment is a mechanical consequence of section order and is the whole
of P10's translation from MIR identities to Wasm indices:

- **Types.** MIR's `types` (recursion groups of GC structs, arrays, and function
  types) are copied to `type_defs` and flattened from index `0`. For each MIR
  function, if its parameter/result signature matches a defined function type in
  that table, that index is reused; otherwise a fresh `FuncType` is appended
  after the defined types. Import signatures, the entry signature, and the
  `cabi_realloc` signature are appended the same way.
- **Functions.** Imports are placed first in MIR import order; defined functions
  follow in MIR order, indexed `import_count + function.id`; the entry and then
  `realloc` are appended. MIR guarantees `FunctionId` equals the function's
  position ([MIR](../fp/mir.md)).
- **Memories and data.** The profile has one memory at index `0`. Data segments
  keep their construction order.
- **Globals.** One mutable `(ref null $string)` global per distinct literal a
  function actually materializes, allocated in `DataId` order from index `0` and
  initialized to `ref.null`. No global is emitted when the module uses no
  literal.

### Declared element segment

When the structurer lowers a `ClosureNew` or another operation it emits
`ref.func`. WebAssembly requires every function referenced by `ref.func` to be
declared. `encode_module` scans all emitted bodies (including the entry and
`realloc`), collects the referenced function indices, sorts and deduplicates
them, and emits a single *declared* element segment (`Elements::Functions`). No
table is installed; the segment exists only to satisfy the declaration rule.

### Data segments and the allocator

String literals are collected once, deduplicated by content, and placed in
passive data segments. Each distinct literal used by the module is interned in
one mutable `(ref null $string)` global: the first evaluation materializes the
GC string with `array.new_data` and stores it, and every later use reads the
same string. A literal is never addressed by MIR and needs no linear buffer
([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)).
The first 16 bytes of memory are a reserved scratch region (`SCRATCH_SIZE`) that
holds the return pointer area of canonical ABI calls; the heap-state segment
follows. When a program imports a function that returns a `list`/`string` or
passes indirect parameters, P10 also emits and exports P9's specified
`cabi_realloc` contract, a general allocator that allocates, frees, and reuses
aligned blocks ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).

### Command entry synthesis

The selected entry declaration must be a zero-argument function returning `Int`
(`i32`). P10 synthesizes a `run` entry with type `() -> i32` whose body calls
`main`, then calls `wasi:cli/exit.exit-with-code` with `main`'s result, then
returns `0`, the canonical `ok` discriminant of the `run` result. The core
module exports this entry under the name `wit-component` expects
(`wasi:cli/run@0.2.12#run`) and exports its linear memory as `memory`. The
component lift and world are described in [WASI platform library](wasi-platform-library.md).

### Rejected alternatives

- **A full Wasm IR mirroring the opcode set.** Rejected: it duplicates
  `wasm-encoder`, must be kept in sync with every proposal, and grows with the
  instruction set instead of the language
  ([DEC-02](../../../decision/DEC-02-thin-structured-wasm-encoding.md)).
- **Encoding bytes directly from MIR.** Rejected: structuring and byte emission
  would be one pass, leaving no independently verifiable structured artifact and
  forcing MIR to carry target structuring hints permanently.
- **Structuring in MIR.** Rejected: MIR is the representation the optimizer and
  verifier operate on; structure is recovered once, late, and only for the
  target.
- **Assigning indices during MIR lowering.** Rejected: final indices depend on
  the complete module skeleton (imports, entry, realloc), which only the encoder
  knows; MIR keeps module-local IDs.

## Algorithms

### Structuring a function

```text
lower_function(mir_fn):
    body = []
    match ControlFlowPlan::build(mir_fn):        // analysis in the CFG topic
        Reducible(region) -> emit_region(region, labels = [], body)
        Dispatcher(blocks) -> emit_dispatcher(blocks, body)
    body.push(LocalGet(mir_fn.result))
    return Function { parameters, locals, body }

emit_region(region, labels, body):
    for (i, unit) in region.units:
        body = [Block { body, span: unit.span }]        // forward-branch target
        active = labels + [Loop(region.header)]
                          + [Block(u.entry) for u in region.units[i+1..].rev()]
        emit_unit(unit, active, body)

emit_unit(unit, labels, body):
    Block(b)             -> emit_body(b, labels, body)
    Loop{header, body=r} -> emit_region(r, labels, loop_body); body.push(Loop(loop_body))

emit_body(node, labels, body):
    emit each instruction of node as leaf ops
    match node.terminator:
        Return  => emit load value; Return
        Jump(t, args) =>
            read all args; for (arg, param) in reverse(zip(args, t.parameters)):
                body.push(LocalGet(local(arg))); body.push(LocalSet(local(param)))
            body.push(Br(depth(t, labels)))
        Branch(cond, then, else) =>
            body.push(LocalGet(local(cond)))
            body.push(BrIf(depth(then, labels))); body.push(Br(depth(else, labels)))
        Switch(v, cases, d) => body.push(BrTable([depth(b,labels) for b in cases],
                                                 depth(d, labels)))
```

Every block is a unit and is emitted exactly once, so no join is required and a
branch whose arms terminate independently is still structured. All jump
arguments are read before any target local is written, which keeps block
parameter permutations correct. Because Wasm forbids reading a non-defaultable
local that was initialized inside an inner structured block, non-parameter
reference locals are declared nullable and the emitter appends
`ref.as_non_null` after each read to restore the MIR value's non-null type. The
full analysis and emission are specified in
[control flow and tail calls](../fp/control-flow-and-tail-calls.md).

### Index assignment

```text
defined = sum(len(group) for group in mir.types)
types   = []                       # function types beyond `defined`
defined_func_index = flattened positions of Func entries in mir.types

for each mir function f:
    if signature(f) in defined_func_index: f.type_index = that index
    else: f.type_index = defined + types.push(signature(f)) - 1

for each mir import i:
    i.type_index = defined + types.push(val_type_sig(i))

entry.type_index  = defined + types.push(() -> i32)
realloc.type_index = defined + types.push((i32,i32,i32,i32) -> i32)

function_index(f) = import_count + f.id
entry_index       = import_count + len(mir.functions)
realloc_index     = entry_index + 1

globals           = [Global{index: i, mutable, (ref null $string), ref.null}
                     for (i, id) in enumerate(sorted used DataIds)]
```

### Section emission

```text
encode_module(module):
    if types non-empty:
        TypeSection: type_defs as rec groups, then function types
    if imports non-empty:
        ImportSection: function imports with their type indices
    if any defined functions:
        FunctionSection: functions, then entry, then realloc
    if memories non-empty:
        MemorySection
    if globals non-empty:
        GlobalSection
    if exports non-empty:
        ExportSection
    refs = sorted-unique RefFunc indices found in all bodies
    if refs non-empty:
        ElementSection: one declared segment over refs
    if any defined functions:
        CodeSection: one body per function, entry, realloc (each ends with `end`)
    if data non-empty:
        DataSection: passive literal segments and the active heap-state segment
    return module bytes
```

### Edge cases

- A module with no code produces no function or code section.
- `entry` requires a WASI CLI target; without it the entry returns `i32` without
  calling `exit-with-code`.
- `realloc` is emitted only when an import returns a `list`/`string`; its free
  pointer and the memory minimum are computed from the end of the string data.
- A `RefFunc` inside a nested `Op::If` is collected recursively, so nested
  closures still declare their code reference.

## Code map

The Wasm target layer is a module tree under
`crates/psrs-backend/src/wasm/`; [IR boundaries](../00-ir-boundaries.md) fixes the
crate-level tree. The implementation must conform to this organization:

```text
wasm/
  mod.rs         Wasm IR: Module, Op (Leaf/If/Block/Loop), Function, Export,
                 DataSegment, and the final index types
  encode.rs      binary encoding: section order, declared element segment, data
  verify.rs      structural verification of the Wasm IR
  lower/
    mod.rs       MIR -> structured Wasm entry points and index allocation
    structure/   structuring and instruction emission
    realloc.rs   cabi_realloc synthesis
    runtime.rs   data segments and strings
```

`wasm/mod.rs` owns the thin encoding and must not depend on MIR. It must define
these types:

- `Module` — the module skeleton of the Model section: imports, function and
  defined types, functions, memories, globals, data segments, exports, optional
  entry, and optional `realloc`.
- `Op` — the structured operation set: `Leaf(wasm_encoder::Instruction)` plus
  the structured regions `If`, `Block`, and `Loop`. Every region shape this
  design names is an `Op` variant; the encoding must not mirror the WebAssembly
  opcode set.
- `Function` — a defined function: symbol, name, final type index, parameter and
  local `ValType`s, body, and span.
- `Export`, with `ExportKind` and `ExportIndex` — the export table, including
  the `memory` export and the synthesized `run` entry.
- `DataSegment` — a passive literal segment or the active heap-state segment.
- `Global` with `GlobalInit` — a module global. The backend only emits mutable
  nullable-reference globals initialized with `ref.null`, one per interned
  literal.
- `TypeIndex`, `FunctionIndex`, `MemoryIndex`, `GlobalIndex`, and `DataIndex` —
  the distinct final index domains. They must not be interchangeable with each
  other or with any MIR identity.

`wasm/lower/` owns the translation from verified MIR and must provide:

```rust
pub fn lower_module(
    module: &mir::Module,
    wasi: &mut abi::WasiRegistry,
) -> Result<Module, Vec<BackendError>>;

pub fn lower_module_with_capabilities(
    module: &mir::Module,
    wasi: &mut abi::WasiRegistry,
    target: TargetCapabilities,
) -> Result<Module, Vec<BackendError>>;
```

`lower_module` must use the default profile; both entry points must re-run
`mir::verify_module_with_capabilities` before translating. `lower/` also owns
**index allocation**: it maps MIR identities to the final index domains as a
mechanical consequence of section order. It must not create, deduplicate, or
reorder types, and it must not choose a representation. Changing a layout,
introducing ABI adaptation, or inferring a missing layout is a P9 decision and
must not be repeated here ([MIR](../fp/mir.md),
[IR boundaries](../00-ir-boundaries.md)).

Within `lower/`, `structure/` owns region recovery and leaf instruction
emission, `realloc.rs` synthesizes and exports `cabi_realloc`, and `runtime.rs`
collects string literals, builds data segments, and allocates the one global per
used literal; `lower/mod.rs` also synthesizes the command entry. The structurer
emits `ArrayNewData` as a `ref.is_null` guard over the literal's global.

`wasm/encode.rs` must provide:

```rust
pub fn encode_module(module: &Module) -> Result<Vec<u8>, Vec<BackendError>>;
```

It owns binary section order, the declared element segment for `ref.func`, and
data emission.

`wasm/verify.rs` must provide:

```rust
pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>>;
```

It checks the structural invariants of the thin IR before any bytes are emitted.
No module under `wasm/` may import the front end or CC.

## Invariants and verification

The thin-IR verifier `wasm::verify_module` checks, before bytes are emitted:

- data segment IDs and indices are unique and each index equals its position;
- memory IDs and indices are unique and deterministic;
- global indices are unique and deterministic, and every global's initializer
  type matches its declared value type;
- every import, function, entry, and `realloc` type index is in range and names
  a function type;
- every export index is in range for its kind;
- every `LocalGet`/`LocalSet`/`LocalTee` is within the function's local count
  (parameters plus locals); every `Call`/`RefFunc` is within the function index
  space; every `GlobalGet`/`GlobalSet` is within the global index space; every
  `CallRef`/`CallIndirect` type index is in range.

After `encode_module`, P11 runs an independent WebAssembly validator configured
from the same capability profile (`validator_for`), then `wasmprinter` produces
WAT ([capability profile](capability-profile.md), [IR boundaries](../00-ir-boundaries.md)).
Structural validation is necessary but not sufficient; observable behavior is
covered by execution tests under the pinned runtime
([WASI platform library](wasi-platform-library.md)). A failure is a compiler bug
or an unsupported program and is reported with a source span, never as a
malformed artifact.

## Worked example

For `main = 7` (a zero-argument `Int` entry), MIR is one function with one block:
`Constant v0 = 7`, then `Return v0`. P10 produces:

- **Types.** No MIR defined types. The `main` signature `() -> i32` becomes
  `FuncType` index `0`. The interned `exit-with-code` signature `(i32) -> ()`
  becomes index `1`, the synthesized entry `() -> i32` index `2`.
- **Imports.** `wasi:cli/exit@0.2.12` / `exit-with-code` at function index `0`,
  type index `1`.
- **Functions.** `main` at function index `1` (`import_count + 0`), type `0`.
- **Entry.** Synthesized at function index `2`, type `2`, body
  `Call(1); Call(0); I32Const(0)`.
- **Exports.** `wasi:cli/run@0.2.12#run` -> function `2`; `memory` -> memory `0`.
- **Memory.** One memory, minimum one page.
- **Code.** `main` is `I32Const(7); LocalSet(0); LocalGet(0); end`; the entry is
  as above; no element or data segment is needed.

Encoding therefore emits type, import, function, memory, export, and code
sections in that order. Adding `log "hello"` would add a string data segment and
a global, so the global section follows the memory section; the literal is
materialized once behind a `ref.is_null` guard (and, because a `list`-returning
import is present, the `cabi_realloc` export is emitted).

## Boundaries and interfaces

- **Input:** a verified `mir::Module`, the `WasiRegistry` that names its imports,
  and an explicit `TargetCapabilities` profile
  ([canonical ABI and WIT](canonical-abi-and-wit.md), [capability profile](capability-profile.md)).
- **Output:** a verified `wasm::Module`, the thin structured encoding.
- **To P11:** the thin module and the capability profile. `encode_module`
  produces core bytes; the componentizer lifts them and the validator checks
  them ([WASI platform library](wasi-platform-library.md)).
- **From MIR:** the entry symbol, the function list and IDs, the type table, the
  canonical import signatures, and the data needed for string segments. P10 may
  map identities to indices but must not choose a representation
  ([MIR](../fp/mir.md), [IR boundaries](../00-ir-boundaries.md)).

## Open questions and future work

- **Multi-value.** `FuncType` admits multiple results, but MIR functions and
  calls have one; the encoder is ready when MIR is
  ([MIR](../fp/mir.md) open questions).
- **Optimization.** [MIR optimization](../opt/mir.md) runs before this
  structurer; the structured encoder performs no semantic optimization.
- **Table-based calls.** `call_indirect` is verified but not produced; closures
  use typed `call_ref`, so no table is emitted.
- **Data segment layout.** Offset assignment and allocator state are fixed here;
  a profile that changes the address type would revisit them
  ([capability profile](capability-profile.md)).

## References

- WebAssembly 3.0 specification: binary format, structured control
  instructions, type and function index spaces, element segments.
- WebAssembly proposals: function references, garbage collection.
- Zakai, *The Relooper*; Gohman, *Beyond Relooper: recursive translation of
  unstructured control flow to structured control flow* (LLVM WebAssembly
  stackifier).
- [DEC-02 — Thin Structured Wasm Encoding](../../../decision/DEC-02-thin-structured-wasm-encoding.md),
  [DEC-09 — GC-Only Language Heap](../../../decision/DEC-09-gc-only-language-heap.md).
- [IR boundaries](../00-ir-boundaries.md),
  [MIR](../fp/mir.md),
  [control flow and tail calls](../fp/control-flow-and-tail-calls.md),
  [canonical ABI and WIT](canonical-abi-and-wit.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md).

## Implementation notes

For every function, the structurer computes reachability, dominators, and
natural loops over the entry-reachable MIR graph, checks loop nesting, and
partitions each region into an ordered list of block and loop units after
removing back edges. One emitter handles every reducible function, with or
without loops: it emits a `Loop` at each natural-loop header and `Block`
continuations for forward targets, including multiple loop exits, and resolves
`Jump`, `Branch`, and `Switch` targets against active labels using computed
depths. MIR carries no `merge_block`, and a branch or switch whose arms have no
common join is still emitted; `common_join` remains only an analysis aid for the
optimizer. Non-parameter reference locals are declared nullable and restored
with `ref.as_non_null` at each read so Wasm accepts values initialized inside
inner structured blocks. A switch maps sparse signed tags to dense unsigned
indices before `br_table`; duplicate constructor patterns still use the
source-order comparison chain.

The thin IR and encoder support structured `If`, `Block`, and `Loop` regions.
Branch-depth verification includes the implicit function label and counts
these regions plus raw `block`, `loop`, `if`, and `end` instructions in `Leaf`
sequences; the synthesized `cabi_realloc` uses such a raw block-and-loop copy
routine. A reachable cyclic SCC with multiple entry blocks selects a
function-level dispatcher: an `i32` local stores the next block index, a
`br_table` selects a nested block label, and each block body updates the state
before branching back to the dispatcher loop. Jump arguments are copied to
target block locals before changing the state; branches and switches update the
state according to their selected successor, including exact comparisons for
sparse signed switch tags. The dispatcher uses only core Wasm control
instructions. `ReturnCall`/`ReturnCallRef` encode `return_call`/`return_call_ref`
when the target enables `tail_call`; the stable profile keeps the flag disabled.
