# D-06 — Backend IR Boundaries and Representation Lowering

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

Define the contract between Typed Core, closure-converted IR (CC), MIR, and the
thin Wasm encoding. In particular, this document assigns runtime
representation, target capability, and platform ABI decisions to one stage so
that CC does not commit the compiler to Wasm GC.

This document complements:

- [D-01](D-01-frontend-and-ir-boundaries.md), which defines the complete pass
  pipeline;
- [D-02](D-02-wasm-lowering.md), which defines Wasm artifact production;
- [D-05](D-05-backend-capability.md), which records target capability status;
- [D-07](D-07-wit-imports-and-std.md), which defines source WIT bindings and
  canonical ABI adaptation; and
- [D-08](D-08-generic-wasm-representation.md), which defines the erased
  fallback for polymorphic values.

The capability level of CC/MIR remains **Partial** until M7 is complete. The
GC/reference/closure path and the WASI scalar/handle/byte-list/nullary-enum slice
are working vertical slices. Under
[DEC-09](../decision/DEC-09-gc-only-language-heap.md), Wasm GC is the only
language-heap strategy; linear memory serves only the byte-oriented canonical ABI
boundary, so the former MVP linear aggregate/array/table-closure planner is
retired. The unified variant representation from
[DEC-08](../decision/DEC-08-target-neutral-variant-representation.md) lowers
through the GC planner. The current canonical WIT adapter lowers its supported
call forms through GC MIR. Direct scalar WIT record parameters have P9 projection
and flattening coverage; WIT flags also have Boolean-record validation and
canonical-word packing tests. Frontend acceptance of record signatures and
Wasmtime execution evidence remain pending. The full CC/MIR scalar family and
typed erased scalar/reference adaptation have execution evidence. Core intrinsic
integration and broader canonical ABI forms remain incomplete; runtime
source-type tests are outside the D-08 erased-value model.

## Backend pipeline

```text
P7  Typed Core
     |
     | semantic lowering, ANF, closure conversion
     v
P8  target-neutral CC IR + representation requirements
     |
     | target selection, representation planning, ABI adaptation, CFG lowering
     v
P9  target-specific MIR + concrete layout table
     |
     | MIR optimization and control-flow structuring
     v
P10 thin structured Wasm encoding
     |
     | binary encoding, validation, component metadata
     v
P11 Wasm/WASI artifact
```

P8 decides what computations and runtime objects are required. P9 decides how
those values and objects are represented for the selected target. P10 may
structure control flow and perform mechanical index translation, but it must
not choose a representation.

## Ownership summary

| Concern | Typed Core | CC IR | MIR | Wasm/platform layer |
| --- | --- | --- | --- | --- |
| PureScript types and constructors | Owns | Lowered to abstract shapes | Absent | Absent |
| Evaluation order | Implicit in expressions | Explicit ANF order | Explicit SSA/CFG order | Preserved |
| Lambdas and free variables | Owns lambdas | Explicit functions and capture lists | Concrete closure operations | Encoded |
| Direct versus closure calls | Semantic call | Explicit | Concrete call convention | Encoded |
| Runtime representation | None | Symbolic requirements only | Owns selected layout | Consumes |
| Wasm value/reference types | None | Forbidden | Owns | Consumes |
| Wasm type table and indices | None | Forbidden | Owns and verifies | Emits |
| Target capabilities | None | Independent | Selection and legality | Final validation |
| WIT interface/function names | External binding metadata | Forbidden | Forbidden | ABI registry/side table owns |
| Canonical ABI signature | None | Forbidden | Canonical imports and adapter code | Names and component metadata |

## P8: target-neutral CC IR

### Contract

CC is an administrative-normal-form, closure-converted representation. It
must express:

- ordered evaluation of named computations;
- direct calls separately from calls through function values;
- lifted functions and their ordered capture lists;
- construction, observation, and mutation requirements for abstract runtime
  values;
- scalar, aggregate, array, variant, closure, and erased-value shapes; and
- structured expression-level control until P9 converts it to a CFG.

CC must not contain:

- `RecGroup`, Wasm `RefType`, `HeapType`, or storage types;
- Wasm type, function, table, memory, local, or data indices;
- `struct.new`, `array.get`, `ref.cast`, `call_ref`, or other target opcodes;
- a choice between GC, tables, or linear-memory allocation;
- canonical ABI pointer/length conventions; or
- WIT package, interface, world, or function names.

Stable semantic IDs such as `SymbolId` may remain. Source spans remain on
operations that can produce diagnostics.

### Representation requirements

CC values use a module-local `ReprId`, not a Wasm value type. A representation
table gives each ID a target-neutral requirement. The implementation uses
`ValueShape`, `Reference`, and `RefShape`; these names are intentionally
distinct from MIR's Wasm types. The model is equivalent to:

```text
ReprId -> Repr

ValueShape = Integer | Boolean | Number | Reference(RefShape)
RefShape = Repr(ReprId) | Aggregate | Erased | Closure(SignatureId)

Repr = Box(ValueShape)
     | Product([ValueShape])
     | Variant([VariantCase])
     | Array(ValueShape)

SignatureId -> ([ValueShape], ValueShape)
VariantCase = { tag, fields: [ValueShape] }
```

`ReprId` describes required behavior, not physical layout. For example,
`RefShape::Closure(signature)` says that a value is callable and has an ordered
capture list; it does not prescribe an environment object or capture array.
`Variant` says which tags and fields must be representable; it does not select
an i31 or a struct hierarchy.

One `Variant` requirement represents one source sum type. Each case has a
stable tag and a field-shape list, so a sum type is one requirement rather than
one requirement per constructor. The concrete case encoding is a P9 decision,
following
[DEC-08](../decision/DEC-08-target-neutral-variant-representation.md):

- the GC planner emits one abstract, non-final `struct` supertype carrying the
  tag plus one final `struct` subtype per case.

The former linear-memory realization as a `{ tag: i32, payload }` record is
retired ([DEC-09](../decision/DEC-09-gc-only-language-heap.md)).

A sum type whose constructors are all nullary keeps the immediate `i32` tag
representation and creates no `Variant` requirement.

Equivalent requirements should be interned so equality is stable and cheap.
The table may preserve links to Core type IDs in lowering-only side data for
diagnostics, but those IDs are not part of representation equality and do not
enter MIR.

### CC operations

CC operations are semantic operations over `ReprId`s. The required families
are:

- constants and target-independent primitives ([D-09](D-09-scalar-and-numeric-lowering.md));
- direct calls by stable symbol and closure calls by `SignatureId`;
- creation of a function value with an explicit capture list;
- capture projection by logical slot;
- product/record construction and field projection by logical field;
- variant construction (`VariantNew`), tag test (`VariantTag`), and field
  projection (`VariantGet`) by case tag and logical field;
- array construction, length, read, and write; and
- representation adaptation between concrete and erased requirements.

The variant operations are the target-neutral way to construct and destructure a
sum type; P9 realizes them with the GC or linear case encoding from DEC-08. The
bootstrap `AssignmentKind` set exposes products, arrays, and representation
adaptation directly, and the completed variant family above; representation
adaptation (`RepresentationTest`/`RepresentationCast`) is reserved for the
erased protocol of [D-08](D-08-generic-wasm-representation.md) and is not used
for constructor dispatch. The CC verifier covers every operation present in that
enum.

An operation may refer to a `ReprId`, `SignatureId`, logical field, capture
slot, or variant tag. It may not refer to a physical field offset or Wasm type
index. This keeps CC independent of the concrete strategy; under
[DEC-09](../decision/DEC-09-gc-only-language-heap.md) the only supported
strategy is:

| Strategy | P9 choice |
| --- | --- |
| Wasm GC | Typed function references plus GC closure/aggregate objects |

A table-slot/MVP language heap is not supported; linear memory is reserved for
the canonical ABI boundary ([D-10](D-10-linear-memory-representation.md)).

### External calls

A CC direct call names only a stable external `SymbolId`. Target binding data
travels beside the CC module in a backend input object, conceptually:

```text
BackendInput {
  cc: CcModule,
  externals: ExternalBindingTable,
  target: TargetCapabilities,
}
```

`ExternalBindingTable` maps a symbol to its source declaration and platform
binding. For the WASI target, the binding contains the WIT name and source
signature required by D-07. P9 resolves these bindings through the ABI
registry, emits canonical calls and adapters for referenced symbols, and keeps
unused runtime imports out of MIR. Consequently, WIT names do not become part
of CC identity, dumps, equality, or verification.

The boundary is checked in both directions. P8 validates that the side table is
a complete projection of Core's WIT externals, and P9 validates that every
binding has exactly one CC external with the same abstract signature. P9
resolves and validates every binding, then projects the ABI registry down to
symbols referenced by lowered MIR calls. An unused valid binding therefore
cannot add a runtime import, while an unsupported or malformed declaration
still receives its source-associated ABI diagnostic.

## P9: representation lowering to MIR

### Responsibilities

P9 consumes CC, external bindings, and an explicit capability profile. It is
the only stage that may:

1. choose scalar and reference value types;
2. select GC object representations and retain linear memory only for the
   canonical ABI boundary;
3. choose closure code/environment layout and calling convention;
4. choose layouts for products, variants, arrays, boxes, and strings;
5. insert allocations, loads, stores, casts, boxing, and unboxing;
6. resolve used platform imports and generate canonical ABI adapters;
7. convert structured CC control into SSA basic blocks; and
8. construct and allocate the concrete MIR type table.

These decisions are made by a representation planner before instruction
selection. The planner produces a total mapping from every reachable `ReprId`
and `SignatureId` to a concrete MIR representation. Recursive requirements are
planned as a group. Failure to represent a requirement under the selected
capability profile is a P9 diagnostic; the emitter must never silently switch
strategies.

### MIR model

MIR is a target-specific, language-independent SSA control-flow graph. It owns:

- typed virtual values;
- basic blocks with typed block parameters;
- explicit jumps, branches, returns, and later general loop edges;
- concrete target operations;
- canonical import signatures;
- concrete aggregate and function types; and
- the selected closure and memory calling conventions.

For the current Wasm target, MIR value and storage types follow the enabled
WebAssembly type system. MIR may therefore contain numeric types, reference
types, defined function/struct/array types, loads/stores, reference operations,
and direct or indirect calls. These are legal in MIR because P9 has already
selected the target representation.

MIR type references use a dedicated `DefinedTypeId` rather than an untyped
`u32`. P9 interns concrete types, creates recursion groups, assigns their
module-local order, and P10 allocates the final Wasm type-index mapping. A raw
index is permitted only at the type-table/encoding boundary, whose API proves
which table it indexes. Defined-type IDs, function IDs, table IDs, memory IDs,
and data IDs are distinct from the final Wasm `TypeIndex`, `FunctionIndex`,
`TableIndex`, `MemoryIndex`, and `DataIndex` forms even though all encode as
`u32`.

### Layout strategy

Under [DEC-09](../decision/DEC-09-gc-only-language-heap.md) there is a single
language-heap planner, selected by `TargetCapabilities`:

- The GC planner uses `struct`, `array`, typed function references, `ref.test`,
  `ref.cast`, and `call_ref`. It realizes a `Variant` as a tag on an abstract
  supertype plus one final case subtype, and realizes `VariantTag` and
  `VariantGet` with `struct.get` and `ref.cast`. The concrete GC layouts are in
  [D-11](D-11-gc-representation-and-evidence.md).
- A profile without GC is rejected with a source-associated diagnostic before
  MIR verification; there is no fallback language heap.

Linear memory is not a layout strategy. It is reserved for the byte-oriented
canonical ABI boundary ([D-10](D-10-linear-memory-representation.md)), and a CC
operation must not branch on the presence or absence of GC.

### ABI boundary

WIT parsing and canonical ABI classification belong to the ABI layer, not to
either long-lived IR. P9 asks that layer to resolve each used external binding
and then materializes the result as ordinary MIR:

- canonical parameters and results in the MIR import table;
- pointer/length expansion, numeric widening/narrowing, and return-area access
  as MIR instructions;
- allocation calls according to the selected memory ABI; and
- ownership/post-return actions when those WIT forms are implemented.

MIR import records contain a stable ABI symbol and a canonical signature. WIT
names, worlds, resource metadata, and component export names remain in the ABI
registry or artifact side data used by P11.

## P10 and P11: thin Wasm target

P10 consumes verified MIR and performs optimizations that preserve MIR types,
then structures the CFG for Wasm. The structured form owns module sections and
structured control regions. Leaf operations may use `wasm_encoder`
instructions as established by
[DEC-02](../decision/DEC-02-thin-structured-wasm-encoding.md).

P10/P11 consume the type table and final type-index mapping allocated by P9.
They may assign final indices for functions and other module entities as a
mechanical consequence of section order. They must not deduplicate or create
types, decide that a closure is a GC struct, change a sum encoding, introduce
canonical ABI adaptation, or infer a missing layout.

## Verification

Every boundary is verified independently. Passing a later verifier does not
replace verification of an earlier representation.

### CC verifier

The CC verifier checks:

- representation and signature IDs exist and their tables are well formed;
- every value is declared once and defined once;
- parameters are available at function entry and assignments use only
  previously available values;
- each constant and primitive has the required operand/result representation;
- each representation adaptation has compatible source and destination
  requirements;
- direct-call arguments and results exactly match the callee signature;
- closure-call arguments and results exactly match its `SignatureId`;
- capture count, order, and representations match the lifted function;
- product fields and array elements match their abstract representation, while
  variant cases are validated in the representation table;
- both branches of a value-producing conditional yield the declared
  representation; and
- no target type, physical layout, numeric Wasm index, or platform name occurs
  in the module.

### MIR verifier

MIR verification has a target-independent entry point and a target-aware entry
point. P9 lowers CC, builds the module, and calls the target-aware entry with
the same `TargetCapabilities` it planned under; P10 reuses it before encoding.
The target-aware entry runs every structural check below and then checks target
legality, so capability rejection happens in MIR verification rather than only
at the encoding boundary.

The MIR verifier checks:

- type and recursion-group well-formedness, including valid subtyping links:
  a supertype must be a declared, non-final type, and the subtype composite must
  satisfy Wasm subtyping (struct fields are a width/depth-compatible prefix with
  invariant mutable fields, an array element is a subtype, and function
  parameters are contravariant and results covariant);
- uniqueness of blocks, values, imports, functions, and concrete type IDs;
- exactly one definition per SSA value;
- definition-before-use within a block and dominance across blocks;
- block parameter and jump argument arity and exact types;
- branch conditions and return values;
- constant and primitive result types, including the scalar operations of
  [D-09](D-09-scalar-and-numeric-lowering.md);
- exact direct, indirect, and reference-call signatures;
- `ref.func` agreement with the referenced function signature;
- reference nullability, casts, aggregate fields, array elements, and closure
  operations against the concrete type table;
- memory address width, access type, alignment, memory identity, and that a
  linear access offset lies within the planned object
  ([D-10](D-10-linear-memory-representation.md));
- canonical import calls against the MIR import table; and
- legality of every type and instruction under the selected target profile.

Passing the MIR verifier does not replace the CC verifier; each boundary is
verified independently.

### Wasm validation

P11 encodes the module and runs an independent Wasm validator configured with
the same capability profile. Component artifacts additionally require WIT
metadata validation and a component-model round trip. Runtime execution tests
remain necessary for observable behavior; structural validation alone is not
sufficient.

## Current implementation and deviations

The current code has a functioning GC-oriented vertical slice for the supported
aggregate/array and closure operation subsets. A second linear-memory
language-heap planner and its MIR pointer-bounds verifier still exist in the
code but are being removed under
[DEC-09](../decision/DEC-09-gc-only-language-heap.md); linear memory is retained
only for the canonical ABI boundary. The current canonical WIT adapter lowers
through GC MIR. Typed erased adaptation executes on GC. Unsupported
`RepresentationTest` operations and canonical ABI forms outside the adapter's
supported subset receive source-associated P9 diagnostics.

| Area | Current implementation | Required state |
| --- | --- | --- |
| Shared types | CC and MIR use separate value/reference/type vocabularies. | **Implemented:** Wasm `ValueType`, `RefType`, `HeapType`, and `RecGroup` are MIR-owned. |
| CC module | `cc::Module` owns only abstract representations and signatures. | **Implemented:** no concrete type table crosses P8. |
| CC operations | Function/closure, product, array, variant, and erased-adaptation operations carry semantic handles and logical slots. `VariantNew`, `VariantTag`, and `VariantGet` handle constructor operations. | **Implemented (M6):** constructor dispatch no longer uses representation tests/casts; those operations are verified as erased adaptation. |
| Layout construction | `cc::layout` interns one `Variant` requirement per sum type; the GC planner realizes its reachable cases. | **Implemented (M6):** GC uses a tag-carrying supertype and case subtypes. The linear tag/payload realization is removed. |
| MIR lowering | P9 resolves abstract handles before MIR verification and lowers the GC path for products, boxes, arrays, variants, closures, typed erased scalar/reference adaptation, and the currently supported canonical WIT call adapters, including nullary enums with validated case order. | **Partial (M7):** a typed erased identity fixture executes for `Int`, `Number`, and concrete references on GC; broader canonical ABI forms and enum runtime evidence remain. The linear language-heap lowering is removed ([DEC-09](../decision/DEC-09-gc-only-language-heap.md)). `RepresentationTest` is not part of the D-08 erased ABI, which has no runtime source-type tags. |
| MIR identities | `DefinedTypeId`, `FunctionId`, and `MemoryId` are used by MIR; P10 owns the conversion to typed final Wasm indices, with data/resource IDs kept distinct from encoded indices. | **Implemented for the current resource set; P10 owns the final index mapping and keeps it typed.** |
| External metadata | `BackendInput::externals` is returned beside CC; `cc::External` retains only a symbol and abstract signature. P8/P9 validate the side-table/CC pairing; P9 resolves all declarations and retains only used ABI symbols in MIR. | **Implemented:** WIT names and source ABI types are backend side-table data; MIR retains only canonical signatures and used ABI symbols. |
| CC verification | `cc::verify` checks declaration/definition order, calls, captures, products, variants, arrays, erased adaptation, and branches. | **Implemented (M6):** variant operations check their tag, field, operands, and result. |
| MIR verification | MIR checks SSA, concrete operation types, structural subtyping, target capability legality, and linear ABI access extents. | **Partial (M7):** access ranges are checked against P9 extents for the canonical ABI boundary; incoming pointers without local provenance and dynamic ranges across CFG joins remain outside the contract. The language-object pointer-bounds analysis is removed with the linear planner. |

The former CC-to-MIR type-table pass-through has been removed. P9 exposes a
planner contract: the GC implementation creates its closure object, erased
capture array, aggregate and variant layouts, and concrete function types. It
walks the CC module's value shapes, operations, called external signatures, and
recursively referenced handles, so unreachable requirements do not become
layouts. The canonical WIT adapter emits its currently supported ABI sequence,
including direct scalar record parameters in backend lowering tests. Typed erased
identity execution covers scalar boxing/unboxing and concrete reference casts on
GC. WIT forms outside the adapter subset receive an explicit P9 diagnostic.
Backend coverage remains **Partial** until the scalar
([D-09](D-09-scalar-and-numeric-lowering.md)), canonical ABI
([D-07](D-07-wit-imports-and-std.md)), and
[linear ABI boundary](D-10-linear-memory-representation.md) slices are complete
and carry execution evidence.

## Migration plan

The migration preserves the current executable slice while moving ownership
one boundary at a time.

### M1 — Complete the target-neutral CC operation contract (implemented)

- Add distinct `ReprId` and `SignatureId` types and interned CC tables.
- Give every CC value a target-neutral `ValueShape`; concrete object
  requirements are referenced through `ReprId`.
- Add table checks and complete operation checks to the CC verifier, including
  calls, closure captures, representation adaptation, products, boxes, arrays,
  and value-producing branches.
- Keep a temporary GC planner that reproduces current layouts exactly.

Exit criterion: CC lowering and verification operate without looking at
`RecGroup`, `RefType`, or a numeric Wasm index, and every operation in the
current CC operation set has an abstract operand/result and handle check.

### M2 — Move layout construction behind a P9 planner contract (implemented)

- Move box, array, record/product, constructor/variant, function, capture-array,
  and closure layout creation out of `cc::layout`.
- Make P9 compute a concrete representation map through a planner contract.
- Provide a GC planner implementation that consumes unchanged
  CC `ReprId`/`SignatureId` requirements. (A second linear-memory planner was
  added here and later retired by DEC-09.)
- Replace raw type-index fields in CC operations with abstract handles and
  logical slots.
- Keep golden MIR/WAT and execution tests for the GC strategy.

Exit criterion: `cc::Module` has no concrete type table, every planner consumes
the same reachable CC handles, and the selected planner owns concrete layout
construction without changing P8.

### M3 — Remove platform bindings from CC (implemented)

- Move `cc::External` binding data into `ExternalBindingTable`.
- Let CC calls retain only `SymbolId` and an abstract signature.
- Resolve and validate external bindings in P9 ABI lowering, while retaining
  only imports referenced by emitted MIR.
- Validate the Core-to-side-table and side-table-to-CC mappings so a missing,
  duplicate, or signature-mismatched binding fails at the owning boundary.
- Preserve the current WIT scalar, handle, and byte-list behavior.

Exit criterion: CC dumps and equality contain no WIT names or canonical ABI
types.

### M4 — Make MIR identities explicit (implemented)

- Replace raw `u32` MIR type references with `DefinedTypeId` and validate the
  ID against the P9-owned concrete type table.
- Give MIR functions and memory operands dedicated IDs, and keep table/data
  resource IDs separate from final encoded index types.
- Make concrete type and function-index allocation deterministic from the
  verified MIR order; P10/P11 only apply typed mappings at the encoding
  boundary.
- Range-check typed defined types, function IDs, memory IDs, final Wasm type,
  function, memory, and data indices before encoding.

Exit criterion: mixing index spaces is unrepresentable in MIR APIs and all
indices are range-checked.

### M5 — Alternative planner retired (reverted by DEC-09)

An MVP linear-memory language-heap planner was built and briefly shared the CC
module with the GC planner. That path could not reclaim memory and would have
required a hand-written collector, so
[DEC-09](../decision/DEC-09-gc-only-language-heap.md) retires it. The `wasm_mvp`
and `linear_memory_wasi_0_2` profiles no longer select a language-heap planner;
linear memory serves only the canonical ABI boundary
([D-10](D-10-linear-memory-representation.md)).

Exit criterion: the backend has a single language-heap planner and no
linear-memory language-object layouts, allocator paths, or pointer-bounds
verifier.

### M6 — Unified variant representation (implemented)

- Add `VariantNew`, `VariantTag`, and `VariantGet` to CC and the CC verifier.
- Build one `Variant` requirement per source sum type; a sum type whose
  constructors are all nullary keeps the immediate `i32` tag and allocates
  nothing.
- Realize the requirement in the GC planner as a tag-carrying abstract
  supertype plus one final case subtype.
- Restrict `RepresentationTest`/`RepresentationCast` to erased adaptation.
- Keep the existing GC execution behavior and add an execution test for a data
  type with fields.

Exit criterion: constructor construction, tag test, and field projection lower
and execute on GC from one CC module. The linear realization was retired by
DEC-09.

### M7 — Complete scalar, canonical ABI, and boundary coverage

- Add the scalar operation slice of
  [D-09](D-09-scalar-and-numeric-lowering.md). **Implemented in the
  backend:** CC/MIR lower the full listed unary and binary vocabulary, with
  Euclidean helpers verified and executed on GC. Extending the Core intrinsic
  mapping remains frontend integration work.
- Expand the canonical ABI of
  [D-07](D-07-wit-imports-and-std.md) to the supported WIT forms and their
  ownership rules; keep linear memory limited to that boundary
  ([D-10](D-10-linear-memory-representation.md)).
- Add Wasmtime execution tests for every promoted capability row.

Exit criterion: acceptance criterion 4 holds and the D-05 capability rows move
from `Partial` to `Implemented` with the four pieces of evidence D-05 requires.

## Acceptance criteria

The corrected boundary is complete only when all of the following hold:

1. No module under `cc` imports the MIR/Wasm type model or stores a Wasm index.
2. CC verification covers every operation's operand and result
   representations, calls, captures, aggregates, variants, arrays, and branch
   results.
3. P9 constructs the concrete layout table from reachable CC requirements for
   an explicit target profile.
4. MIR verification covers SSA, CFG, concrete types, subtyping, references,
   aggregates, variants, calls, memory, imports, and target legality.
5. WIT names live only in external-binding/ABI/artifact data, never in CC or
   MIR.
6. The existing GC closure, aggregate, string, and WASI tests still validate
   and execute.
7. There is one language-heap planner; linear memory carries no language
   objects.

Criteria 1–3, 5, 6, and 7 are met. Criterion 4 is met for the current
instruction set and grows with M7. The capability audit stays **Partial** until
the M7 slices have execution evidence.

## Non-goals

- CC is not a portable serialized interchange format.
- MIR is not target-neutral and does not preserve PureScript type semantics.
- The thin Wasm encoding is not another optimization IR.
- [DEC-09](../decision/DEC-09-gc-only-language-heap.md) selects GC as the single
  language heap and reserves linear memory for the canonical ABI boundary; the
  design does not support a second language-heap strategy.
- Supporting the GC planner does not by itself claim complete Wasm proposal,
  Component Model, or WASI service coverage.
