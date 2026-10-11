# Handoff: Lower the Delimited-Control Executor

**Date:** 2026-10-11  
**Branch:** `feat/effect-handlers` (based on `stdlib/vendor-core-libraries`)  
**Scope:** Continue the compiler lowering for the `Prim.Control` delimited-control
surface. The `return`/`run` path works end to end; the `perform`/`bind` executor and
`handle` are not lowered.  
**Status:** Partial. Frames and effect summaries are in place; the executor needs a
design decision recorded below before it can be emitted.

## Objective

Implement **[DEC-19](../../decision/DEC-19-compiler-generated-resumable-frames.md)**'s
compiler-generated resumable-frame mechanism for the `Prim.Control` surface, so that
library code can define algebraic-effect operations and handlers on top of it. The
normative designs are:

- [Library-Owned Algebraic Effects and Handlers](../../design/backend/fp/algebraic-effects-and-handlers.md)
  (library model; the operations, rows, handlers live in the standard library).
- [Delimited Control and Resumable Frames](../../design/backend/fp/delimited-control-and-resumable-frames.md)
  (the compiler mechanism: the surface, the frame representation, the executor, the
  runtime boundary, the comparison with stack switching). **Read this first.**
- [Algebraic Effects: Proposed Acceptance Checklist](../../implementation/backend/algebraic-effects.md)
  (AE-*/AC-* evidence requirements; all still Unverified).

The scope decision: **the type system gains no effect tracking or effect reasoning**;
effect requirements are ordinary library types. The carrier and primitives are
compiler-owned; operations, rows, and handlers are library code. The mechanism is
confined below Typed Core.

## What is implemented

### Frontend surface (committed `1633dd88`)

`Prim.Control` (a compiler-reserved interface) exposes:

```text
Control f a                  -- opaque carrier, kind (Type -> Type) -> Type -> Type
return  : a -> Control f a
bind    : Control f a -> (a -> Control f b) -> Control f b
perform : f a -> Control f a
run     : s
        -> (forall x. s -> f x -> (x -> Control f a) -> (s, Control f a))
        -> Control f a
        -> (s, a)
handle  : s
        -> (forall x. s -> f x -> (x -> Control u a) -> (s, Control u a))
        -> (forall x r. u x -> (f x -> r) -> (g x -> r) -> r)   -- project (Church sum)
        -> (forall x. g x -> u x)                               -- inject
        -> Control u a
        -> Control g (s, a)
```

`run` is the closed state-threading fold; `handle` is the open eliminator that
removes one effect and leaves a residual. Touch points:

- `crates/psrs-hir/src/primitives/control.rs` — the `Control` carrier declaration
  (kind, representational roles).
- `crates/psrs-hir/src/intrinsic/mod.rs` — `ControlReturn=92 … ControlHandle=96`,
  `ALL`, reserved-slot assertion.
- `crates/psrs-hir/src/intrinsic/registry.rs` — `IntrinsicCategory::Control`,
  descriptors, and the schemes (`control_run`, `control_handle`, `record` helper).
- `crates/psrs-hir/src/intrinsic/effects.rs` — `run`/`handle` are `may_trap`+`may_write`.
- `crates/psrs-hir/src/compiler_interface.rs` — the `Prim.Control` interface
  (`return`/`bind`/`perform`/`run`/`handle`).
- `crates/psrs-hir/src/lib.rs` — `TypeId::CONTROL`.

### Backend representation and the `return`/`run` path (committed `64a27de8`)

- **`Control f a` is an opaque aggregate.** `crates/psrs-backend/src/cc/mod.rs`
  inserts `HirTypeId::CONTROL` into `aggregate_types`, so `scalar_type` needs no
  change. `crates/psrs-backend/src/cc/layout/mod.rs` reserves the shared **frame**
  representation and stores it in `TypeLayout::control_type`:

  ```text
  Variant {
    Return  [Erased]            -- tag 0
    Perform [Erased]            -- tag 1, the request only (no continuation)
    Bind    [Erased, Erased]    -- tag 2, an inner computation and a continuation
  }
  ```

- **`control_type` is threaded** through `LoweringContext`/`FunctionLowerer`
  (`crates/psrs-backend/src/cc/lower/mod.rs`).
- **`return`/`perform`/`bind`** lower to frames in
  `crates/psrs-backend/src/cc/lower/intrinsic.rs::lower_control` (a `VariantNew`
  with erased fields via `lower_array_element`).
- **`run`** lowers inline (`lower_control_run`): read the tag, and for a `Return`
  frame build the `(state, value)` record and return it; every other tag traps.
- `crates/psrs-backend/src/target_intrinsics/mod.rs` adds
  `Implementation::Control` and maps the five intrinsics to it.

### Executor plumbing (committed `d8fc2508`)

`generated_signatures: Rc<RefCell<HashMap<SymbolId, Signature>>>` is shared by the
lowering context and the function lowerer, and the per-function verifier resolves
direct calls against the declared signatures **plus** these. This is the plumbing a
lowering-synthesized executor function needs. **No lowering registers a signature
yet.**

### Tests

`crates/psrs-driver/tests/control_surface.rs` (5 tests, all passing):
`control_surface_typechecks`, `control_surface_handles_one_effect`,
`control_surface_rejects_result_type_confusion`,
`control_surface_requires_a_rank_two_handler`, `executes_a_returning_computation`.

`executes_a_returning_computation` compiles a `return`/`run` program, runs it under
`wasmtime`, and asserts **exit 42**. The exact program:

```purescript
module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
program :: C.Control Op Int
program = C.return 42
result :: { _1 :: Int, _2 :: Int }
result = C.run 0 handler program
  where
  handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
  handler s (Op k) cont = { _1: s, _2: cont (k s) }
main :: Int
main = result._2
```

## How to run

The on-disk `/Users/biu/Projects/psrs-stdlib` differs from `stdlib.lock.json`, so the
driver requires the locked revision via `PSRS_STDLIB_ROOT`. A materialized copy of
locked revision `1e01310` is at
`/private/var/folders/f2/h3z3zsy51xq3kr06707r9ct40000gn/T/opencode/psrs-stdlib-1e01310`
(re-materialize with `git archive` if it is gone; do not change the lock or the
package).

```sh
export PSRS_STDLIB_ROOT=/private/var/folders/f2/h3z3zsy51xq3kr06707r9ct40000gn/T/opencode/psrs-stdlib-1e01310
cargo build --workspace
cargo fmt --all --check
cargo clippy -p psrs-backend -p psrs-driver -p psrs-hir --all-targets -- -D warnings
cargo test -p psrs-backend --lib            # 422 tests
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test control_surface
```

`wasmtime` 49.0.2 is installed. All of the above were green at `d8fc2508`.

## Remaining work

The next slice is the **executor**: lower `ControlRun` (and `ControlHandle`) to a
generated function that drives frames, and implement `ControlPerform`/`ControlBind`
so they build frames carrying a **continuation**, not just the request.

### The blocking design decision

The executor and the continuation closures it needs are lowering-synthesized
functions. `AssignmentKind::FunctionRef`/`IndirectCall` require a `SignatureId` from
the `RepresentationTable`, which is **immutable during CC lowering** (only the layout
phase has `&mut`). The `generated_signatures` table added in `d8fc2508` holds CC
`Signature` values (parameter/result shapes) for the *verifier*, but not the
`SignatureId` the emitter needs. Two ways forward:

- **(a) Make the representation table extensible during lowering** (`Rc<RefCell<…>>`).
  Direct but broad.
- **(b) Pre-declare erased signatures and representations in layout** and emit one
  generic erased executor. **This is the intended path.** Reserve in
  `crates/psrs-backend/src/cc/layout/mod.rs`:
  - `control_cont_signature : [Erased] -> Erased` (a continuation),
  - `control_executor_signature : [Erased, Erased, Erased] -> Erased`
    (state, handler, computation → an erased `(state, value)` pair),
  - `control_pair : Product[Erased, Erased]` (the `(state, value)` result),
  and add them to `TypeLayout`, threaded like `control_type`.

### Executor semantics

The freer-monad fold for the closed `run` is:

```text
exec h s (Return a)        = (s, a)
exec h s (Perform op cont) = let (s', next) = h s op cont in exec h s' next
exec h s (Bind inner g)    = exec h s' (continue inner g)
```

At the erased level: `TagSwitch` on the computation's tag; `Return` builds the
`control_pair`; `Perform` calls the rank-2 handler (erased
`[Erased,Erased,Erased]->Erased`) and recurses on its result; `Bind` evaluates the
inner computation and composes the continuations. Recurrence is a self tail call,
which MIR loopification turns into a `loop`. The call site boxes `s`/`handler`/
`computation`, calls the executor, then unboxes the pair and converts each field to
the concrete `(s, a)` record. A single generic executor is enough (dedupe by symbol).

The frame representation must change: **`Perform` must carry its continuation**, and
`bind` must compose continuations (`bind (Perform op k) g = Perform op (\x -> bind (k x) g)`),
so `perform op` needs an identity continuation (`\x -> Return x`) and `bind` needs a
composition closure — both synthesized with `FunctionRef` using the pre-declared
signatures. This is where the synthesis work lives.

### Additional touch points

- `crates/psrs-backend/src/cc/lower/intrinsic.rs` — rewrite `lower_control_run`;
  implement the `perform`/`bind` continuation closures; generate the executor.
- `crates/psrs-backend/src/cc/mod.rs` — carry the new pre-declared handles.
- `crates/psrs-backend/src/cc/lower/mod.rs` / `lambda/mod.rs` — thread them; the
  executor is appended via `self.generated`.
- `crates/psrs-backend/src/target_intrinsics/tests.rs` — the control intrinsics are
  `Implementation::Control` now (they are no longer expected `Unsupported`).
- `crates/psrs-backend/src/mir/opt/effects.rs` — classify the executor call as
  effectful so a suspension is never erased.

### Not started

- `ControlHandle` (the open eliminator): not lowered.
- The standard-library wrapper (`Run r a`, `handleState`, …): not written. The
  library can be authored once the executor works; the surface typechecks today.
- Verifier/optimizer obligations for the executor and for suspension observability.
- Runtime-side design (the executor is guest code; `psrs-runtime` carries no
  continuation for the software form — see the design's "executor and runtime
  boundary").

## Known limitations / honest gaps

- Only the `return` path of `run` executes. `perform`/`bind` build frames that trap
  at `run` until the executor lands.
- The frame representation currently has no continuation slot in `Perform`; it must
  be revised as described.
- `handle` is unreachable.
- The locked standard library is stale relative to the Effect/State removal, so the
  default root cannot compile library-dependent programs; the control tests use
  self-contained sources and the locked revision via `PSRS_STDLIB_ROOT`.

## Workflow notes

- The user has been reviewing and committing topical commits as work lands. Local
  history on `feat/effect-handlers` is not pushed; the four commits above are the
  current state. Do not rewrite pushed history (none is pushed).
- AGENTS.md governs the repository: keep changes topical, keep compiler
  representations separate, do not narrow the design to close gaps, and report
  unsupported cases honestly.
