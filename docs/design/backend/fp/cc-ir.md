# CC IR: ANF and Closure Conversion

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** the [functional core](../../frontend/semantics/functional-core.md) calculus,
administrative normal form, and closure conversion; read
[IR boundaries](../00-ir-boundaries.md) and [functional core](../../frontend/semantics/functional-core.md)
first.  
**Summary:** CC is the target-neutral administrative-normal-form and
closure-converted backend representation. It fixes evaluation order, lifts
lambdas into functions with explicit capture lists, records abstract
representation requirements for the values a program creates, and keeps every
operation independent of the concrete Wasm layout that [MIR](mir.md) selects.

## Scope

This document owns the CC model, the ANF and closure-conversion contract, the
representation-requirement model (`ReprId`/`SignatureId`/`ValueShape`), the CC
operation families, the external-binding boundary, and the CC verifier. It
specifies what CC must express and must never contain.

It does not own the Core terms it lowers (see [functional core](../../frontend/semantics/functional-core.md)),
the concrete runtime layout or Wasm type table (see [MIR](mir.md)), the pattern
decision algorithm (see [pattern matching](pattern-matching.md)), the erased
representation protocol (see [polymorphism and erasure](polymorphism-and-erasure.md)),
the scalar operator definitions (see [scalars and primitives](scalars-and-primitives.md)),
or the WIT/canonical-ABI binding rules (see
[canonical ABI and WIT](../wasm/canonical-abi-and-wit.md)). Dictionary
elaboration is specified in [type classes and dictionaries](type-classes-and-dictionaries.md).

## Background

**Administrative normal form (ANF).** In ANF every intermediate computation is
bound to a name exactly once, and every argument of a call is a value or a
variable. Flanagan, Sabry, Duba, and Felleisen (*The Essence of Compiling with
Continuations*, 1993) show that this is the canonical administrative form for a
call-by-value language. ANF makes evaluation order explicit without leaving the
expression world: a CC function is an ordered list of `Assignment`s, not yet a
control-flow graph.

**Closure conversion.** A nested lambda may refer to variables bound outside
it. Closure conversion turns it into a top-level function whose extra
*closure* parameter carries those free variables, the *captures*, and turns the
lambda expression into a function value pairing the code with the captures.
Steele (1978) and Appel (*Compiling with Continuations*, 1992) are the standard
references. CC keeps captures explicit and ordered; the physical closure object
is chosen later by [MIR](mir.md).

**Evaluation order.** Because Core is strict, the order in which operands are
evaluated is observable as soon as effects exist. CC makes Core's defined order
explicit: `lower_value` lowers operands in source order and emits one `Assignment`
per result, so reading the assignment list top to bottom is reading the
evaluation order. Structured `If` keeps its branch assignments nested rather
than flattened, because only one branch runs.

**Target-neutral requirements.** CC must be representation-independent, but it
cannot forget that a value is an integer, a product, or a closure. It therefore
carries *requirements*, not layouts: `ReprId` names an abstract shape, and a
separate table records the shape. The chosen concrete layout is a pure P9
function of those requirements ([MIR](mir.md)), which is what lets CC be
verified without mentioning Wasm.

## Model

### Module, function, and assignments

```text
Module     = { name: String, externals: [External],
               representations: RepresentationTable,
               functions: [Function], entry: Option(SymbolId), span: TextRange }
External   = { symbol: SymbolId, signature: Option(Signature) }
Function   = { symbol: SymbolId, name: String, parameters: [ValueId],
               values: [ValueDecl], assignments: [Assignment],
               result: ValueId, result_type: ValueShape, span: TextRange }
Assignment = { destination: ValueId, kind: AssignmentKind, span: TextRange }
ValueDecl  = { id: ValueId, ty: ValueShape }
```

`ValueId` is the backend-side local identity from
`crates/psrs-backend/src/types.rs`. The Rust model is
`crates/psrs-backend/src/cc/mod.rs`: `Module`, `External`, `Function`,
`Assignment`, `AssignmentKind`, and `ValueDecl`. `entry` is a stable `SymbolId`,
not a source name. Source spans are retained on every assignment because it can
produce a diagnostic.

A function's `parameters` must be the first `ValueDecl`s and are the only values
available at entry; every assignment defines exactly one new value. `ArraySet`
produces the updated array as a distinct value after a clone, preserving the
source operation's purity. The list of
assignments is ordered so that the reader sees evaluation order.

### Representation requirements

CC values use a module-local `ReprId`, never a Wasm value type. The
representation table gives each ID a target-neutral requirement. The
implementation names are `RepresentationTable`, `ReprId`, `SignatureId`,
`ValueShape`, `Reference`, `RefShape`, `Representation`, `Signature`, and
`VariantCase` (`crates/psrs-backend/src/cc/representation.rs`). The model is
equivalent to:

```text
RepresentationTable = { representations: [Representation], signatures: [Signature] }

ReprId      -> Representation
SignatureId -> Signature

ValueShape  = Integer | Boolean | Number | String | Reference(Reference)
Reference   = { nullable: bool, heap: RefShape }
RefShape    = Repr(ReprId) | Aggregate | Erased | Closure(SignatureId)

Representation = Box(ValueShape)
               | Product([ValueShape])
               | Variant([VariantCase])
               | Array(ValueShape)

Signature    = { parameters: [ValueShape], result: ValueShape }
VariantCase  = { tag: u32, fields: [ValueShape] }
```

`String` is a semantic CC shape, distinct from numeric `Integer`; P9 alone maps
it to the current target's linear-memory address. This distinction lets the CC
verifier reject arithmetic on strings and verify WIT string arguments without
putting a pointer layout in CC.

`ReprId` describes required behavior, not physical layout. For example,
`RefShape::Closure(signature)` says a value is callable and has an ordered
capture list; it does not prescribe an environment object or capture array.
`Variant` says which tags and fields must be representable; it does not select
an i31 or a struct hierarchy. `RefShape::Aggregate` is a reference to an
aggregate value without committing to a concrete constructor or object
representation, used for the closure parameter of lifted functions and while a
value's concrete requirement is not yet needed, and `RefShape::Erased` is the
erased protocol of [polymorphism and erasure](polymorphism-and-erasure.md).
`ReprId`s and `SignatureId`s are distinct index spaces from `ValueId` and from
MIR's `DefinedTypeId`.

`RepresentationTable::reserve` allocates a stable `ReprId` before its
`Representation` is known, so recursive references can be built, and `set`
fills it in; this is how a type that mentions itself is represented.
`RepresentationTable::add_signature` appends a signature, and P8 interns equal
signatures in `cc/layout/functions.rs` so equivalent requirements share a
`SignatureId`. The table may preserve links to Core type IDs in lowering-only
side data for diagnostics, but those IDs are not part of representation
equality and do not enter MIR.

One `Variant` requirement represents one source sum type. Each case has a
stable tag and a field-shape list, so a sum type is one requirement rather than
one requirement per constructor. The concrete case encoding is a P9 decision,
following [CC IR](cc-ir.md):
the GC planner emits one abstract, non-final `struct` supertype carrying the tag
plus one final `struct` subtype per case. The former linear-memory realization
as a `{ tag: i32, payload }` record is retired
([DEC-09](../../../decision/DEC-09-gc-only-language-heap.md)). A sum type whose
constructors are all nullary keeps the immediate `i32` tag representation and
creates no `Variant` requirement at all; it lowers to `Constant` tags and
integer comparisons.

### CC operations

CC operations are semantic operations over `ReprId`s, spelled by
`AssignmentKind` (`crates/psrs-backend/src/cc/mod.rs`):

- scalar constants and conversions: `Constant`, `NumberConstant`,
  `StringConstant`, `Primitive`, `Unary` (operators in `cc/scalar.rs`,
  semantics in [scalars and primitives](scalars-and-primitives.md));
- calls: `DirectCall` by stable `SymbolId`, `IndirectCall` through a closure
  value with a `SignatureId`, and `FunctionRef` creating a function value with
  an explicit capture list;
- closure operations: `ClosureGetCapture` by logical capture slot;
- products and records: `ProductNew`, `ProductGet` by logical field;
- variants: `VariantNew`, `VariantTag`, `VariantGet` by case tag and logical
  field;
- arrays: `ArrayNew`, `ArrayLen`, `ArrayGet`, `ArrayClone`, `ArraySet`;
- representation adaptation: `RepresentationTest`, `RepresentationCast`;
- structured control: `If`, which nests a then and an else assignment list.

An operation may refer to a `ReprId`, `SignatureId`, logical field, capture
slot, or variant tag. It may not refer to a physical field offset or a Wasm type
index. `VariantTag` produces an `Integer`; the P9 planner realizes the case
encoding. `ProductNew` and `ProductGet` also serve `Box` representations, which
is how the erased scalar boxes of
[polymorphism and erasure](polymorphism-and-erasure.md) are built and read.
`RepresentationTest`/`RepresentationCast` are reserved for the erased protocol
and are never used for constructor dispatch; `verify_erased_adaptation` rejects
a test or cast whose source is not erased and whose target is not erased.

`If` is the only control construct in CC. It is not a CFG: the branch
assignment lists are nested and produce a value, and P9 converts them into
basic blocks. This is deliberate — CC stays expression-shaped until the last
moment so that evaluation order and capture structure can be verified before
control becomes a graph.

### External calls

A CC direct call names only a stable external `SymbolId`. Target binding data
travels beside the CC module in a backend input object, conceptually:

```text
BackendInput    = { cc: CcModule, externals: ExternalBindings }
ExternalBindings = { imports: [ExternalBinding] }
ExternalBinding  = { symbol: SymbolId, interface: String,
                     function: String, signature: Option(SourceSignature) }
```

The Rust names are `BackendInput`, `ExternalBindings`, and `ExternalBinding`
(`crates/psrs-backend/src/bindings.rs`); `ExternalBindings` is the concrete side
table. Its `imports` map a symbol to its source declaration and platform binding. For the
WASI target the binding contains the WIT interface and function names and the
source signature required by the [canonical ABI](../wasm/canonical-abi-and-wit.md).
P9 resolves these bindings through the ABI registry, emits canonical calls and
adapters for referenced symbols, and keeps unused runtime imports out of MIR.
Consequently, WIT names do not become part of CC identity, dumps, equality, or
verification.

The boundary is checked in both directions. P8's `ExternalBindings::validate_core`
validates that the side table is a complete projection of Core's WIT externals.
P9's `ExternalBindings::validate_cc` validates that every binding has exactly
one CC external with the same abstract signature, matching source and CC
signatures through `cc::signature_matches_source`. P9 resolves and validates
every binding, then projects the ABI registry down to symbols referenced by
lowered MIR calls. An unused valid binding therefore cannot add a runtime
import, while an unsupported or malformed declaration still receives its
source-associated ABI diagnostic.

### Invariants

- Representation and signature IDs exist and their tables are well formed;
  `RefShape::Repr`/`Closure` handles resolve.
- Every value is declared once and defined once; parameters are declared and
  available at entry; uses refer only to values already available.
- Capture indices read from one closure are contiguous and agree in shape.
- Function references match their target's parameter list (closure parameter
  first) and capture shapes.
- Variant tags are unique within a `Variant`; variant fields match the declared
  case.
- `If` branches have equal result shapes and the destination has that shape.
- No target type, physical layout, numeric Wasm index, or platform name occurs
  anywhere in the module.

## Design

### What CC must express

- ordered evaluation of named computations;
- direct calls separately from calls through function values;
- lifted functions and their ordered capture lists;
- construction, observation, and mutation requirements for abstract runtime
  values;
- scalar, aggregate, array, variant, closure, and erased-value shapes; and
- structured expression-level control until P9 converts it to a CFG.

### What CC must not contain

- `RecGroup`, Wasm `RefType`, `HeapType`, or storage types;
- Wasm type, function, table, memory, local, or data indices;
- `struct.new`, `array.get`, `ref.cast`, `call_ref`, or other target opcodes;
- a choice between GC, tables, or linear-memory allocation;
- canonical ABI pointer/length conventions; or
- WIT package, interface, world, or function names.

Stable semantic IDs such as `SymbolId` may remain. Source spans remain on
operations that can produce diagnostics.

### The chosen split

CC owns *semantics and order*; P9 owns *layout*. The strategy table below is the
only supported strategy under [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md):

| Strategy | P9 choice |
| --- | --- |
| Wasm GC | Typed function references plus GC closure/aggregate objects |

A table-slot language heap is not supported; linear memory is reserved for
the canonical ABI boundary ([linear ABI boundary](../wasm/linear-memory-and-canonical-abi-boundary.md)).
CC names neither of these.

### Rejected alternatives

- **Concrete layout in CC.** Rejected: it would force one target strategy into
  the semantic representation and make CC depend on the Wasm type system, which
  [DEC-01](../../../decision/DEC-01-distinct-ir-boundaries.md) forbids.
- **One `Variant` requirement per constructor.** Rejected: a sum would lose its
  identity as one type; the tag-carrying supertype and one-case-per-subtype
  model needs the whole case set in one place
  ([CC IR](cc-ir.md)).
- **Keeping expression trees instead of ANF.** Rejected: evaluation order and
  intermediate values would stay implicit, and the CC verifier could not check
  definition-before-use.
- **Flattening `If` into a graph in CC.** Rejected: P9 already owns CFG
  construction, and nested branch assignment lists let the verifier check both
  branch result shapes before control is a graph.
- **Putting WIT names or canonical ABI pointers in CC.** Rejected: it would make
  the platform ABI part of CC equality and dumps; the side table keeps the
  platform boundary out of the IR.
- **Using `RepresentationTest`/`RepresentationCast` for constructor dispatch.**
  Rejected: dispatch is `VariantTag`/`VariantGet` over abstract cases; the
  representation operations exist only for the erased protocol and the verifier
  enforces that restriction.

## Algorithms

### ANF lowering

```text
lower_value(expr):
    case expr of
      Integer n            -> dest = fresh(Integer); emit Constant(n)
      Number s             -> dest = fresh(Number);  emit NumberConstant(s)
      Char c               -> dest = fresh(Integer); emit Constant(c as i32)
      Local(l)             -> return locals[l]
      Global(g)            -> return a function value for g (see below)
      Primitive(op,l,r)    -> l' = lower_value(l); r' = lower_value(r)
                              dest = fresh(ty); emit Primitive(op, l', r')
      Record/Array/Field/...
                           -> lower operands in source order, then emit one
                              Product*/Array* operation
      Application          -> lower callee, then arguments left-to-right, then
                              emit DirectCall or IndirectCall
      Let(bs, body)        -> lower each binding in order, extend locals, then
                              lower body
      If(c,t,e)            -> c' = lower_value(c)
                              then = lower_list(t); else = lower_list(e)
                              dest = fresh(ty); emit If(c', then, t', else, e')
      Lambda               -> closure conversion (below)
      Case                 -> pattern decision (below)
```

Operands are always lowered before the operation that consumes them, so the
assignment list is a topological order of the value graph and reading it top to
bottom is the evaluation order.

### Closure conversion

```text
lower_lambda(lambda):
    signature = function_types[lambda.ty]
    captures  = free locals of the body, in first-use order, deduplicated
    generate a top-level function f with parameters
        [closure_parameter, bound_parameter, ...]
    inside f:
        for (index, capture) in captures:
            v = fresh(shape_of(capture))
            emit ClosureGetCapture(closure_parameter, index) -> v
            bind capture to v
        result = lower_value(body)
    closure = fresh(Closure(signature))
    emit FunctionRef(f, signature, captures) -> closure
    if lambda.ty is a generic function: emit RepresentationCast to Erased
    return closure
```

Capture order is the order `collect_captures` discovers free locals while
walking the body, deduplicated on first sight; the lifted function reads them
back by the same index, and `verify_capture_layout` requires the indices to be
contiguous and shape-consistent. Every top-level declaration also gets a
generated closure wrapper (`make_wrapper`) so a `Global` can be used as a
function value without a separate calling convention.

### Partial application and erased adapters

When a `Global` is applied to fewer arguments than its signature has, P8
generates a wrapper closure that captures the supplied arguments and calls the
original function with the remaining parameters appended; this is what makes
`runEffect (log "message")` lower without a special calling convention
([effects](effects.md)). When a concrete function value crosses a polymorphic
function boundary, `adapt_erased_function_value` builds an adapter closure with
the erased signature that captures the original, boxes/unboxes each parameter,
calls the concrete closure, and boxes/unboxes the result
([polymorphism and erasure](polymorphism-and-erasure.md)).

### Pattern decision and case lowering

`lower_case` first selects the decision compiler
(`cc/case/decision.rs`), then lowers each selected alternative. The current
construction compiles ordered alternatives into conditional `If` chains: for
each constructor branch it compares the scrutinee tag with the case tag and
nests the next branch in the `else`. Newtypes are erased to their field;
aggregate cases use `VariantTag`/`VariantGet`; records use `ProductGet`; nested
constructor and record patterns test a projected sub-value. The scrutinee is
evaluated once. The complete target — a decision DAG with multi-way `Switch` on
tags and matrix-based exhaustiveness and redundancy — is
[pattern matching](pattern-matching.md); the abstract case operations it emits
are already in place.

### Verification

`verify_module` builds a signature environment from every function and external,
then `verify_function_inner` checks each function: value declarations are
unique, parameters are declared and distinct, captures are contiguous, and
`verify_assignments` walks the assignments maintaining the set of available
values and checking every operand and destination shape. `verify_table` checks
the representation table. The [Invariants and verification](#invariants-and-verification)
section lists what is checked.

## Code map

The CC IR is owned by the `cc` module tree under
`crates/psrs-backend/src/cc/`. The tree is:

```text
cc/
  mod.rs             `Module`, `Function`, `Assignment`, `AssignmentKind`, lowering entry
  representation.rs  `ReprId`, `ValueShape`, `Reference`, `RefShape`, `Representation`, `RepresentationTable`
  layout/            P8 layout requirement construction (Core types -> abstract shapes)
  lower/             P8 lowering: ANF, closure conversion, patterns
  verify/            CC verifier
```

`cc/mod.rs` provides the model and the two stage entry points:

- `Module`, `External`, `Function`, `Assignment`, `AssignmentKind`, and
  `ValueDecl`;
- `lower_module(module: psrs_core::Module) -> Result<BackendInput, Vec<BackendError>>`,
  which extracts the default external bindings and delegates below;
- `lower_module_with_bindings(module: psrs_core::Module, bindings: ExternalBindings) -> Result<BackendInput, Vec<BackendError>>`,
  which validates Core and the bindings, constructs the representation
  requirements, and lowers every declaration.

`cc/representation.rs` provides the target-neutral requirement model:
`RepresentationTable`, `ReprId`, `SignatureId`, `ValueShape`, `Reference`,
`RefShape`, `Representation`, `Signature`, and `VariantCase`.
`RepresentationTable` must provide `reserve` (allocate a stable `ReprId` before
its shape is known), `set` (fill it in), and `add_signature`.

`cc/layout/` provides the Core-to-CC shape and signature construction that the
lowering consumes.

`cc/lower/` provides the ANF traversal, closure conversion, and pattern lowering
that emit assignments.

`cc/verify/` provides
`verify_module(module: &Module) -> Result<(), Vec<BackendError>>` and the
per-function and representation-table checks it drives.

The `cc` modules must not import the MIR or Wasm type model. A source file under
`cc/` may use `crate::types` and the Core input types, and may carry the
`ExternalBindings` passed to it; it must not name a Wasm value type, a MIR type,
a physical offset, or a Wasm index.

## Invariants and verification

The CC verifier (`verify_module`, `verify_table`, `verify_function_inner`,
`verify_assignments`) checks:

- representation and signature IDs exist and their tables are well formed;
  `Representation::Variant` cases are non-empty and tags unique;
- every value is declared once and defined once;
- parameters are available at function entry and assignments use only
  previously available values;
- closure capture indices are contiguous and read with consistent shapes;
- each constant and primitive has the required operand/result representation;
  `StringConstant` produces `String`, numeric primitives reject it, and a
  `Boolean` constant is `0` or `1`;
- each representation adaptation has compatible source and destination
  requirements, and adaptation is used only on erased values;
- direct-call arguments and results exactly match the callee signature;
- closure-call (`IndirectCall`) arguments and results exactly match its
  `SignatureId`, and the callee value has that closure shape;
- capture count, order, and representations match the lifted function;
- product fields and array elements match their abstract representation, while
  variant cases are validated in the representation table;
- `ArraySet`'s destination is a fresh array value; its source array remains
  available and unchanged;
- both branches of a value-producing conditional yield the declared
  representation; and
- no target type, physical layout, numeric Wasm index, or platform name occurs
  in the module.

Failure is a compiler bug or an unsupported program. It is reported with the
operation's source span and owning module, never as invalid output.

## Worked example

Take this Core declaration:

```purescript
main :: Int
main = let y = 41 in (\x -> x + y) 1
```

`lower_function` peels no outer lambda, so `main` has no parameters. The
straight-line traversal produces the following CC assignments, in evaluation
order:

```text
main:
    v0 = Constant 41
    v1 = FunctionRef G(S){captures: [v0]}
    v2 = Constant 1
    v3 = IndirectCall v1 : S(v2)
    result = v3
```

The lambda is closure-converted into the generated function `G`, which captures
`y` as slot `0`. Generated functions number their values independently from
`main`:

```text
G(closure: Aggregate = c, x: Integer = x):
    v0 = ClosureGetCapture(c, 0)     // the captured y
    v1 = Primitive(IntAdd, x, v0)
    result = v1
```

`S` is the `SignatureId` of `Int -> Int`, whose `Signature` is
`{ parameters: [Integer], result: Integer }`. The `FunctionRef` destination has
shape `Reference { nullable: false, heap: Closure(S) }`; the `IndirectCall`
checks that `v1` has that shape and that `v2` has the signature's `Integer`
parameter shape. P8 also generates `main_closure_wrapper`, a one-parameter
function whose single assignment is `DirectCall(main, [])`, so `main` can be
passed as a function value. P9 later turns `Integer` into `i32`, the
`FunctionRef` into a GC closure, and the `IndirectCall` into a `call_ref`; none
of that layout appears here.

A variant is similar: `Just (x + 1)` becomes a `Primitive` followed by
`VariantNew(variant_repr, case = 1, [sum])`, and a `case` becomes
`VariantTag`/`VariantGet` (or a tag `Constant` plus `IntEq` for a nullary sum)
inside `If` assignments. See [MIR's worked example](mir.md) for the SSA form.

## Boundaries and interfaces

- **Input.** A verified Core module plus the external binding side table
  (`ExternalBindings`). P8 calls `ExternalBindings::validate_core` and
  `Module::verify` before lowering.
- **Output.** A verified `cc::Module` carried in a `BackendInput` together with
  the bindings that P9 needs. CC is target-neutral: it names no layout.
- **To P9 (MIR).** CC supplies representation requirements, function
  signatures, and ordered assignments. P9 chooses the layout, builds the Wasm
  type table, and converts structured `If` into a CFG; it must not invent a
  requirement CC did not state.
- **From Core.** Core types and names are consumed here; nothing below CC
  depends on Core `TypeId`s except lowering-only diagnostic side data.

## Open questions and future work

- **Decision DAG and `Switch`.** The design target is the full matrix compiler;
  the abstract operations it needs already exist
  ([pattern matching](pattern-matching.md)).
- **General effects.** The partial-application and partial-application-closure
  machinery is the current effect lowering; the monadic/CBPV representation may
  change how effect values are built ([effects](effects.md)).
- **Dictionaries.** CC already accepts products and closures, so no new
  representation is required when constraints land; the elaboration happens at
  Core ([type classes and dictionaries](type-classes-and-dictionaries.md)).
- **Open rows.** Closed records only; row polymorphism changes how
  `record_types` requirements are built, not the operations.

## Implementation notes

The current code has `Constant`, `NumberConstant`, `StringConstant`,
`Primitive`, `Unary`, `DirectCall`, `FunctionRef`, `IndirectCall`,
`ClosureGetCapture`, `RepresentationTest`, `RepresentationCast`, `ProductNew`,
`ProductGet`, `VariantNew`, `VariantTag`, `VariantGet`, `ArrayNew`, `ArrayLen`,
`ArrayGet`, `ArrayClone`, `ArraySet`, and `If`. Pattern lowering produces nested
`If` chains rather than a decision DAG, and local recursive `Let` groups are not
yet lowered. Nothing in this document depends on those temporary shapes.

## References

- Flanagan, Sabry, Duba, and Felleisen, *The Essence of Compiling with
  Continuations* (1993).
- Steele, *Rabbit: A Compiler for Scheme* (1978).
- Appel, *Compiling with Continuations* (1992).
- [DEC-01](../../../decision/DEC-01-distinct-ir-boundaries.md),
  [CC IR](cc-ir.md),
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).
- [IR boundaries](../00-ir-boundaries.md), [functional core](../../frontend/semantics/functional-core.md),
  [MIR](mir.md), [polymorphism and erasure](polymorphism-and-erasure.md).
