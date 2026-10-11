# Handoff: Delimited-Control Lowering

**Date:** 2026-10-11
**Branch:** `feat/effect-handlers`
**Starting commit:** `2026238f`; the checkout was clean before this continuation.
**Scope:** Typed `Prim.Control` surface, closed/open executors, and persistent bind
sequencing. The continuation changes below are uncommitted.
**Status:** Partial topic. All five current primitives are lowered; complete AE/AC acceptance remains outstanding.

## Governing contracts

- [DEC-19](../../decision/DEC-19-compiler-generated-resumable-frames.md).
- [Delimited Control and Resumable Frames](../../design/backend/fp/delimited-control-and-resumable-frames.md).
- [Library-Owned Algebraic Effects and Handlers](../../design/backend/fp/algebraic-effects-and-handlers.md).
- [Acceptance checklist](algebraic-effects.md).

The type system gains no effect tracking or effect reasoning. The compiler owns
only the carrier and generic mechanism below Typed Core; concrete operations,
rows, and handlers remain library code. The executor is guest code, and
`psrs-runtime` does not inspect control frames.

## Committed starting state

- `1633dd88`: the `Control f a` carrier and the checked schemes for
  `return`, `bind`, `perform`, `run`, and `handle`.
- `64a27de8`: inert `Return`, `Perform`, and `Bind` frames; only `run` of a
  `Return` frame executed, while every other frame trapped.
- `d8fc2508`: a shared direct-call signature table for lowering-generated
  functions. It did not provide emitter `SignatureId` handles.
- `2026238f`: the original lowering handoff.

The original five `control_surface` tests passed at this checkpoint, including
Wasmtime exit 42 for `run (return 42)`. A new request runtime reproducer compiled
but trapped with `unreachable` (exit 134).

## Current lowering

The representation table stays immutable during CC lowering. Layout predeclares
all machine representations and the executor signature; the continuation uses
the existing unary erased callable-slot protocol. Verifier direct-call signatures
are registered from those same declarations.

```text
Computation = Return(value) | Perform(request) | Bind(inner, continuation)
            | Resume(value, savedSequence)
            | Handle(state, handler, project, inject, inner)
Sequence    = Empty | Cons(continuation, rest) | Append(left, right)
executor(state, handler, computation, pendingSequence) -> erased (state, value)
```

`Bind` is inert. The executor pushes its continuation and processes its inner
computation. `Return` consumes one pending continuation. At `Perform`, the
handler receives a resumption capturing the persistent pending sequence; calling
it builds `Resume` data. It does not recursively execute the saved work. `Resume`
joins saved and caller sequences in that order, with loop-driven normalization.

After a handler returns its next state/computation, the executor clears its old
pending sequence. Only the returned computation chooses whether to resume,
abandon, or replay saved work. All frame/sequence nodes remain immutable, so
multiple resumptions share the saved chain safely. A bare `Perform` needs only
the request; its identity continuation is the empty sequence. This supersedes the
original handoff's proposed recursive composition closures and mandatory
continuation field on every `Perform` node.

Concrete state, results, handlers, and bind closures cross the existing payload
conversion boundary. The result pair uses the canonical erased `_1`/`_2` record
protocol. Closed/open executors and generated resumption/projection functions are shared across all
`run` sites in the linked module. Self tail calls become MIR loop back edges.
Conservative ordinary call effects preserve ignored-result interpretation; no
control-specific optimizer classification was introduced.

### Code owners

- `crates/psrs-backend/src/cc/layout/control.rs`: predeclared machine layouts and
  signatures.
- `crates/psrs-backend/src/cc/lower/control/`: source primitive lowering, synthesis,
  CC construction helpers, shared driver reductions, and open forwarding.
- CC lowering context and lambda children: shared generated functions/signatures.
  Nested functions resolve generated executor calls during verification.
- `crates/psrs-backend/src/mir/lower/variant.rs`: the general CC-to-MIR variant
  construction contract. A concrete common variant result is constructed at its
  case type before widening; the direct regression covers nullary and
  field-bearing cases at abstract/common result shapes.
- `crates/psrs-driver/tests/control_surface.rs` and `control_runtime.rs`: source
  rejection and mandatory value-sensitive execution evidence.

## Open-handler continuation

`handle` is an inert node. The open executor shares closed-driver reductions,
but returns residual computation data. Its Church-sum projection selects handled
requests or unchanged residual payloads. A residual bind closure captures the
inner state, handler and pending sequence and constructs a fresh `Handle` around
`Resume`; it does not recursively interpret the sequence. Outer replay preserves
independent copies of saved inner state, and outer abandonment discards the inner
remainder. The caller's pending bind stays outside the inner delimiter.

The current machine retains the original union computation, so `inject` is
strictly evaluated and stored but is not called to reconstruct a request. The
normative design records this contract. Native stack use may depend on handler
nesting depth, not on the number of requests at fixed depth.

`control_open_runtime.rs` adds executed open-handler, forwarding, nested-handler,
aggregate, replay, trap and GC evidence. The GC case requires an observed completed
collection in the runtime log before claiming success. Source tests reject an
incompatible residual functor/injection and a monomorphic projection.

## Validation

The locked standard-library revision `1e01310` is materialized at:

```text
/private/var/folders/f2/h3z3zsy51xq3kr06707r9ct40000gn/T/opencode/psrs-stdlib-1e01310
```

Set `PSRS_STDLIB_ROOT` to that directory. Do not modify the lock or substitute the
working standard-library checkout. Run runtime evidence with
`PSRS_REQUIRE_WASMTIME=1`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test control_surface --test control_runtime --test control_open_runtime
```

Final validation passed formatting, full clippy, 423 backend library tests, and
38 control tests with mandatory Wasmtime. Full workspace acceptance still fails:
the open-continuation `--no-fail-fast` run and archived `2026238f` baseline have the same
9 failing targets and 176 failing test names. The acceptance checklist records
the decomposition, logs, tested cases, and remaining obligations.
Compile acceptance, executed runtime checks, and official source comparison are
separate claims.

## Remaining work

- Implement library wrappers in the independent `psrs-stdlib` repository.
- Complete generic deep/shallow wrappers, one-shot, lifetime/cancellation,
  foreign capture-boundary, foreign-resource GC, and official differential evidence.
  Ordinary GC capture recovery during suspension has executed evidence.
- Formalize selective conversion beyond the monadic surface. This closed executor
  does not establish direct-style capture or arbitrary user recursion safety.
- Extend verification evidence for complete frame identity and lifetime claims.
  Current generic verifiers check ordinary shapes, calls, tags, and control flow;
  they trust checked answer types and the generated machine's sequencing rules.
- Repair the separate mismatch between the locked package's old Effect bindings
  and the compiler's removed Effect/State mechanism. A locked-package failure is
  not evidence about the self-contained control programs.

No complete AE/AC requirement is marked Verified by these slices. All current
`Prim.Control` primitives lower and execute; one-shot and expired-boundary
guarantees still require their API and representation contracts.
