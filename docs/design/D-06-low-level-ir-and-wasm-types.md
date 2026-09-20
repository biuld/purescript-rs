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

The capability level of CC/MIR remains **Partial** until the migration in this
document is complete. The existing GC, reference, closure, and WASI canonical
ABI paths are working vertical slices, but they do not yet implement the
required ownership boundary.

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
an i31, a struct hierarchy, or a linear-memory tag/payload record.

Equivalent requirements should be interned so equality is stable and cheap.
The table may preserve links to Core type IDs in lowering-only side data for
diagnostics, but those IDs are not part of representation equality and do not
enter MIR.

### CC operations

CC operations are semantic operations over `ReprId`s. The required families
are:

- constants and target-independent primitives;
- direct calls by stable symbol and closure calls by `SignatureId`;
- creation of a function value with an explicit capture list;
- capture projection by logical slot;
- product/record construction and field projection by logical field;
- variant construction, tag test, and field projection;
- array construction, length, read, and write; and
- representation adaptation between concrete and erased requirements.

An operation may refer to a `ReprId`, `SignatureId`, logical field, capture
slot, or variant tag. It may not refer to a physical field offset or Wasm type
index. This lets the same CC module lower to at least these representation
strategies:

| Strategy | Possible P9 choice |
| --- | --- |
| Wasm GC | Typed function reference plus GC closure/aggregate objects |
| Table closure | Function-table slot plus an environment handle |
| Linear memory | Pointer to compiler-defined object headers and payloads |
| MVP-only | `i32` handles, `call_indirect`, and linear-memory objects |

The table lists required alternatives, not current implementation claims.

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

## P9: representation lowering to MIR

### Responsibilities

P9 consumes CC, external bindings, and an explicit capability profile. It is
the only stage that may:

1. choose scalar and reference value types;
2. select GC, table, linear-memory, or hybrid object representations;
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

MIR type references should use a dedicated ID newtype rather than an untyped
`u32`. P9 interns concrete types, creates recursion groups, assigns their
module-local order, and allocates the final Wasm type-index mapping. A raw
index is permitted only in that type-table module, whose API proves that it
indexes the MIR type table. Defined-type IDs, function IDs, table IDs, memory
IDs, data IDs, and their encoded index forms are distinct types even if all
encode as `u32`.

### Layout alternatives

The representation planner is selected by `TargetCapabilities`:

- A GC planner may use `struct`, `array`, typed function references,
  `ref.test`, `ref.cast`, and `call_ref`.
- A table planner may use a function table and `call_indirect`, with captures
  represented separately.
- A linear-memory planner may assign object headers, alignment, byte offsets,
  and allocator operations.
- An MVP-only planner must reject or lower every non-MVP requirement before
  MIR verification.

These planners may share analyses and layout caches, but a CC operation must
not branch on the chosen planner.

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
- product fields, variant cases, and array elements match their abstract
  representation;
- both branches of a value-producing conditional yield the declared
  representation; and
- no target type, physical layout, numeric Wasm index, or platform name occurs
  in the module.

### MIR verifier

The MIR verifier checks:

- type and recursion-group well-formedness, including valid subtyping links;
- uniqueness of blocks, values, imports, functions, and concrete type IDs;
- exactly one definition per SSA value;
- definition-before-use within a block and dominance across blocks;
- block parameter and jump argument arity and exact types;
- branch conditions and return values;
- constant and primitive result types;
- exact direct, indirect, and reference-call signatures;
- `ref.func` agreement with the referenced function signature;
- reference nullability, casts, aggregate fields, array elements, and closure
  operations against the concrete type table;
- memory address width, access type, alignment, and memory identity;
- canonical import calls against the MIR import table; and
- legality of every type and instruction under the selected target profile.

The recent dominance, duplicate-definition, constant/result, `ref.func`, and
aggregate/reference checks close important verifier gaps. They do not make the
CC-to-MIR ownership boundary complete.

### Wasm validation

P11 encodes the module and runs an independent Wasm validator configured with
the same capability profile. Component artifacts additionally require WIT
metadata validation and a component-model round trip. Runtime execution tests
remain necessary for observable behavior; structural validation alone is not
sufficient.

## Current implementation and deviations

The current code has a functioning GC-oriented vertical slice. The first three
ownership migrations are now implemented; the remaining deviations are listed
explicitly below:

| Area | Current implementation | Required state |
| --- | --- | --- |
| Shared types | CC and MIR use separate value/reference/type vocabularies. | **Implemented:** Wasm `ValueType`, `RefType`, `HeapType`, and `RecGroup` are MIR-owned. |
| CC module | `cc::Module` owns only abstract representations and signatures. | **Implemented:** no concrete type table crosses P8. |
| CC operations | Function/closure, product, array, and representation-adaptation operations carry semantic signatures, handles, and logical slots. Closure operations do not carry environment or box layouts. | **Implemented for the current operation set.** |
| Layout construction | `cc::layout` interns requirements; `mir::layout::PlannedLayout` realizes them. | **Implemented for the GC planner; alternative planners remain future work.** |
| MIR lowering | P9 plans and resolves every representation/signature handle before MIR verification. | **Implemented for the current GC slice.** |
| External metadata | `BackendInput::externals` is returned beside CC; `cc::External` retains only a symbol and abstract signature. | **Implemented:** WIT names and source ABI types are backend side-table data. |
| CC verification | `cc::verify` checks declarations, definition order, constant shapes, and exact direct-call shapes. | It checks the complete abstract operation type/shape contract, including indirect calls, captures, products, arrays, and branches. |
| MIR verification | MIR now checks SSA ordering/dominance and important concrete operation types. | Complete remaining CFG, subtype, memory, capability, and ABI checks. |

The former CC-to-MIR type-table pass-through has been removed. The current P9
planner creates its GC closure object, erased capture array, aggregate layouts,
and concrete function types itself. Backend coverage remains Partial because a
second planner has not yet demonstrated that the abstract CC contract is
sufficient, and CC operation verification is not yet complete.

## Migration plan

The migration preserves the current executable slice while moving ownership
one boundary at a time.

### M1 — Introduce typed abstract handles (representation part implemented)

- Add distinct `ReprId` and `SignatureId` types and interned CC tables.
- Give every CC value a target-neutral `ValueShape`; concrete object
  requirements are referenced through `ReprId`.
- Add table checks and initial operation checks to the CC verifier; complete
  operation coverage remains an explicit follow-up.
- Keep a temporary GC planner that reproduces current layouts exactly.

Exit criterion for the representation portion: CC lowering and the current
verifier operate without looking at `RecGroup`, `RefType`, or a numeric Wasm
index. Full operation compatibility checking remains criterion 2 below.

### M2 — Move layout construction to P9 (implemented for GC)

- Move box, array, record/product, constructor/variant, function, capture-array,
  and closure layout creation out of `cc::layout`.
- Make P9 compute a concrete representation map and MIR type table.
- Replace raw type-index fields in CC operations with abstract handles and
  logical slots.
- Keep golden MIR/WAT and execution tests for the GC strategy.

Exit criterion: `cc::Module` has no concrete type table and P9 can rebuild the
existing GC layout from CC requirements.

### M3 — Remove platform bindings from CC (implemented)

- Move `cc::External` binding data into `ExternalBindingTable`.
- Let CC calls retain only `SymbolId` and an abstract signature.
- Resolve and validate external bindings in P9 ABI lowering, while retaining
  only imports referenced by emitted MIR.
- Preserve the current WIT scalar, handle, and byte-list behavior.

Exit criterion: CC dumps and equality contain no WIT names or canonical ABI
types.

### M4 — Make MIR identities explicit

- Replace raw `u32` type references with dedicated MIR ID types.
- Separate defined-type, function, table, memory, data, and final Wasm index
  domains.
- Make type-index allocation deterministic and owned by P9, with P10/P11 only
  applying the recorded mapping.

Exit criterion: mixing index spaces is unrepresentable in MIR APIs and all
indices are range-checked.

### M5 — Prove target independence

- Add a second representation planner: table closures or linear-memory
  aggregates are the preferred first proof.
- Add an MVP-only capability profile that either lowers each CC requirement or
  reports a P9 diagnostic before emission.
- Run the same CC fixtures through both planners and compare observable
  behavior where both are supported.

Exit criterion: at least one non-GC strategy consumes unchanged CC, and the
CC/MIR capability rows may be reconsidered independently rather than being
promoted together.

## Acceptance criteria

The corrected boundary is complete only when all of the following hold:

1. No module under `cc` imports the MIR/Wasm type model or stores a Wasm index.
2. CC verification covers every operation's operand and result
   representations, calls, captures, aggregates, arrays, and branch results.
3. P9 constructs the concrete layout table from reachable CC requirements for
   an explicit target profile.
4. MIR verification covers SSA, CFG, concrete types, references, aggregates,
   calls, memory, imports, and target legality.
5. WIT names live only in external-binding/ABI/artifact data, never in CC or
   MIR.
6. The existing GC closure, aggregate, string, and WASI tests still validate
   and execute.
7. One alternative planner consumes the same CC without modifying P8.

Until criteria 1–5 are met, the architecture boundary remains incomplete.
Until criterion 7 and the corresponding execution evidence exist, backend
capability coverage remains **Partial**.

## Non-goals

- CC is not a portable serialized interchange format.
- MIR is not target-neutral and does not preserve PureScript type semantics.
- The thin Wasm encoding is not another optimization IR.
- This design does not select a permanent sum, closure, allocator, or string
  layout; those are target-profile choices implemented by P9 planners.
- Supporting a new planner does not by itself claim complete Wasm proposal,
  Component Model, or WASI service coverage.
