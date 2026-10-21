# Official Effect.Exception

**Feature:** [F-02](../../../feature/F-02-portable-programs.md)\
**Status:** Stable contract\
**Decision:** [DEC-24](../../../decision/DEC-24-effect-exception-error-representation.md)\
**Acceptance:** [EX-01..EX-18](../../../implementation/backend/effect-exception.md)

## Scope and ownership

The independent stdlib implements the complete official synchronous
`Effect.Exception` public surface. Ordinary PureScript owns Error construction,
message/name/stack/Show, throw/try wrappers and delayed native requests. The
compiler owns two checked runtime exports and native tag transport. No source
module or declaration name selects an implementation. Existing Effect/ST
interpretation and the Control exception bridge retain their contracts.

## Error representation and source fidelity

`Error` is a constructor-hidden `newtype Error = Error String`, declared in
`Effect.Exception` with its unchanged non-orphan Show instance. A nullary opaque
foreign datatype represents a WIT resource handle and is unsuitable for a
language-heap error. The adaptation preserves every public signature and export;
its pinned source, hashes, exact diff and runtime observations belong to
`psrs-stdlib/docs/effect-exception.md`. The rank-2 private stackImpl is retained.
Official throw and try bodies remain byte-identical, as do
Effect.Exception.Unsafe and MonadError's official Effect instances.

Only the message is retained. `name` returns Error; `stack` returns Nothing.
Show follows the official no-stack `toString` branch: an empty message prints
Error, otherwise Error: followed by a space and the exact message. Unicode and
empty messages are preserved. Stack-bearing JS Show output is deliberately
outside this equality claim.

## Checked source and representation contracts

The provider `psrs:runtime-exception` has three exports:

```purescript
checkThrows :: forall a. (Unit -> a) -> Boolean
throw :: forall a. String -> a
catch :: forall a. (Unit -> a) -> (String -> a) -> a
```

The first is the existing tag-agnostic observer. Throw/catch are the only new
primitives. Every source declaration is checked before reachability pruning or
ABI erasure, including unused declarations. Catch requires the body's,
handler's and overall result types to agree using Core's shared type relation.
CC publication rechecks operand shapes and result agreement. P9 retains checked
binding identity, signature and payload layout; missing facts are errors.
The provider has no host ABI and creates no Wasm imports.

A module with a live throw/catch call plans one local `psrs:source-error` tag
at TagId(0) with one non-null GC String payload. CheckThrows-only modules and
unused throw/catch declarations add no tag. Unused physical signatures are
still checked. The budget appends its distinct payload-less stack-exhaustion
tag after the module's tags. Final Wasm type-index remapping includes the
String tag signature; no resource handle or erased Any substitutes for it.

## Lowering and dynamic behavior

Throw discharges to a Throw terminator carrying the message. It defines no
normal destination. P9 stops its assignment sequence and appends function,
if/switch and protection exits only when the block remains open. After the CFG
is complete, P9 uses the shared try-region partition to resolve its matching
local handler; None means no enclosing clause catches the tag. The
[initial probe](../../../implementation/backend/effect-exception-probe-1.md)
records why no Instruction::Throw fallback is needed.

Catch creates one Try with a protected body, a single named Catch clause and a
result-parameterized join. The body closure takes fabricated erased Unit; the
handler block receives the real String payload and calls the handler closure.
Returning body and handler paths supply the same typed destination at the join.
There is no catch-all clause and no verifier or structurer relaxation.

The library constructs both requests with Native.perform. Construction is
inert; the catch is armed only while Native.run interprets the action. A handler
runs outside its own protection, so a handler throw reaches an outer catch.
Repeated interpretation repeats effects. Source effects, mutation and output
already performed are not rolled back. Uncaught source errors reach the command
entry's existing catch-all and exit 1 with empty output.

## Composition and deliberate boundaries

Catch selects source-error only. Stack-exhaustion and unrelated native tags
propagate unchanged; JavaScript's catchable RangeError stack overflow is an
explicit target divergence. Engine traps, including assertion and invalid
resumption traps, remain non-catchable.

Escaping source errors use the Control bridge: runner roots expire and acquired
owners complete before the same exception token is rethrown. Native catches
retain dynamic scope; they do not attach to saved continuations. Ordinary
PSRS.Control.Except handlers still handle only their typed requests, while
Effect.Exception.try explicitly converts a selected native source error to
Either Error a. No generic effect row or native-to-arbitrary-error conversion
is introduced. Disabled exception profiles reject live calls at the binding's
source span; there is no host fallback.

## Open questions

Source-level stack traces need a separate shadow-call-stack design. Entry-level
stderr diagnostics, asynchronous Aff behavior, engine-trap recovery, automatic
resource release and transactional rollback remain separate topics. This topic
does not reproduce JS deferred-module-initialization ReferenceError behavior or
upstream's tail-call classification, and does not close BE-21 or the corpus gate.
