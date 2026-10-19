# DEC-23 — The WebAssembly Exception Channel and the Stack-Exhaustion Budget

**Status:** Accepted  
**Date:** 2026-10-18  
**Amended:** 2026-10-19 (the channel's corpus acceptance revised — see the
amendment below).  
**Amends:** [DEC-05](DEC-05-wasmtime-feature-set.md), the clause that keeps
exceptions among the permanently disabled proposals.  
**Updates the premise of:** [DEC-19](DEC-19-compiler-generated-resumable-frames.md)'s
stack-switching rejection, which cited the disabled `exceptions` profile flag.  
**Governed by:** [F-02](../feature/F-02-portable-programs.md) and
[exception handling and stack exhaustion](../design/backend/fp/exception-handling-and-stack-exhaustion.md).

## Context and constraints

Upstream `Test.Assert.checkThrows` is a foreign whose contract is "true if the
argument throws when applied", and the official suite uses it to observe the
JavaScript engine's stack-overflow `RangeError`
(`tests/upstream/passing/TCOMutRec.purs`, whose `ntco1`–`ntco4` are
deliberately not tail-call-optimizable). The target has no exception surface:
`checkThrows` has no implementation, and stack exhaustion is a Wasm trap that
no catch clause can observe. The blocker is recorded in
[D-04](../design/D-04-suite-roadmap.md) (#95 blocked on `checkThrows`;
`assertThrows` omitted rather than approximated).

The exception-handling proposal is no longer a moving target: WebAssembly 3.0
standardized it in September 2025, and the pinned wasmtime 48 accepts it
(through the CLI's `-W exceptions` option, enabled in the default feature
set). Under [DEC-05](DEC-05-wasmtime-feature-set.md), runtime acceptance is not
adoption: a proposal needs a lowering, profile validation, regression tests,
and observable execution evidence, behind a profile flag with fallback or
rejection behavior.

Two constraints shape the design. First, traps are not exceptions: stack
exhaustion must be *converted* — a compiler-managed budget must raise a
catchable compiler-defined exception before the engine's own limit, the same
pre-conversion pattern as V8's StackGuard or the JVM's
`StackOverflowError`. Second, the conversion needs headroom: the budget must
sit strictly below the engine limit, and the engine limit must be pinned where
the harness controls it, because a budget calibrated against an unknown host
default is no contract at all.

## Decision

Adopt the **exception-handling proposal minimally**, behind the existing
`exceptions` capability flag, together with a **compiler-managed
stack-exhaustion budget** under the same flag.

- The channel's surface is compiler-defined tags, `try_table`, the
  `catch`/`catch_all` clauses, and `throw`, confined to MIR, the structured
  Wasm encoding, and the structuring machinery — never above Typed Core
  ([IR boundaries](../design/backend/00-ir-boundaries.md)).
- The budget is guest code the compiler emits: one mutable global depth
  counter, a per-function compile-time frame weight, a prologue check that
  throws a payload-less compiler tag (`psrs:stack-exhaustion`) before the
  engine's limit, and epilogue/tail-call decrements. Catch handlers restore a
  saved stack watermark, because EH unwinding skips epilogues. The budget is a
  profile-fixed constant; the runtime's only role is pinning
  `wasmtime::Config::max_wasm_stack` (or the CLI equivalent) so the relation
  `budget < engine limit` is deterministic, with the CLI default recorded for
  the board protocol rather than relied on.
- The first (and today only) consumer is the `Test.Assert.checkThrows`
  primitive, implemented as a compiler-managed runtime binding
  (`psrs:runtime-exception#checkThrows`) that lowers into an inline try region.
- **No fallback ABI.** A module that needs the channel is rejected with a
  source-associated diagnostic when the profile disables `exceptions`; the
  stable default profile stayed disabled until the adoption evidence landed,
  and flipping the default is the adoption's final profile revision.

### Amendment to DEC-05

DEC-05's disabled list is narrowed: `exceptions` moves from "remain disabled"
to *adopted minimally behind its flag*, with the full adoption obligations of
that decision — a lowering, profile validation, regression tests, and
observable execution evidence enabled *and* disabled under the pinned
wasmtime 48 baseline. SIMD, threads, multi-memory, memory64, wide arithmetic,
async components, and WASI 0.3 remain disabled, and every other clause of
DEC-05 (feature confinement to the lowest representations, the GC
representation, validator construction from the profile, no implicit
fallbacks) is retained. WebAssembly 3.0's standardization of exception handling
makes this an adoption of a standard, not of a preview proposal, so the
baseline-revision caution in DEC-05's consequences applies with correspondingly
less force.

This also updates the *premise* of
[DEC-19](DEC-19-compiler-generated-resumable-frames.md)'s stack-switching
rejection, which noted that stack switching "depends on the `exceptions`
feature, and the pinned profile disables `exceptions`". That dependency now
exists in the profile. The rejection itself stands unchanged on its own basis:
stack switching is still not merged into the core specification as an
execution mechanism DEC-19 relies on, and the delimited-control design uses
compiler-generated heap frames. Adoption of `exceptions` does not reopen
[DEC-19](DEC-19-compiler-generated-resumable-frames.md); it removes one stale
reason from its reasoning.

## Alternatives considered

- **Forward engine traps to `checkThrows` (rejected).** Traps cannot be
  caught in WebAssembly; "observe a trap" would mean terminating the guest, and
  `assertThrows` must continue execution afterwards.
- **A linear-memory counter instead of a global (rejected).** Linear memory is
  the byte-oriented WASI ABI boundary, not language state; a global is bounds
  check-free and keeps the allocator's regions untouched
  ([linear memory boundary](../design/backend/wasm/linear-memory-and-canonical-abi-boundary.md)).
- **A compile-time maximum-depth computation (rejected).** Non-tail recursion
  depth is input-dependent (`ntco1 100` returns, `ntco1 100000` throws) and
  predicting it statically is the halting problem's boundary; non-tail
  recursion within the stack limit is upstream's actual semantics.
- **Budget checks only on recursion-cycle members (rejected as default).**
  Dynamic dispatch through typed function references forces a conservative
  call graph, and an under-approximation leaves a deep path unchecked — the
  engine traps and `checkThrows` silently observes the wrong thing. Recorded
  as an optimization with a stated soundness obligation, not the default.
- **Adopting the full proposal surface now, including `exnref` and
  re-raise (rejected for this slice).** The minimal contract's consumer never
  re-raises; each addition must earn its own evidence
  ([DEC-05](DEC-05-wasmtime-feature-set.md)).

## Consequences

- The backend gains the channel (tags, `try_table` structuring, `throw`) and
  the budget mechanism as design obligations with an implementation checklist
  under `docs/implementation/backend/`; acceptance is board evidence, not a
  local test: the expected result of the final slice is TCOMutRec moving from
  its current P8 blocker to passing — moving the recorded runtime board from
  287 to 288 of 340 program groups (a number that must be re-measured by the
  board protocol, never assumed) plus enabled/disabled execution evidence.
  *Revised by the 2026-10-19 amendment above: TCOMutRec's corpus assertion is
  upstream-implementation-specific and stays a recorded failure row
  (re-attributed to a runtime trap); the board is re-measured with the
  profile flip and the number recorded in D-04.*
- Stack exhaustion stops being an unobservable trap on the target: deep
  non-tail recursion becomes a catchable compiler exception, matching
  upstream's observable contract at a shallower depth. The approximation is
  recorded in the design, not hidden.
- The [control machine](DEC-21-control-machine-construction-boundary.md) is
  unaffected: its resumable frames are heap-allocated and consume no native
  stack per resumption, so the budget counter and the machine do not double
  account; machine handlers do not yet intercept channel exceptions, and that
  composition is explicitly future work.
- Out of scope, recorded in the design: the FFI-excluded `4179.purs` (TDZ
  initialization errors with verbatim messages), re-pointing `assert'`
  failures from the storage trap to the channel, `Effect.Exception`'s full
  surface, pattern-match failures as observable exceptions, and any
  runtime-configurable budget.
- The depth global and the weight constants become part of the artifact
  contract under the enabled profile; the artifact assumes a pinned engine
  stack limit above the budget, which the design's coordination fixture
  validates under the pinned wasmtime.

## Amendment — 2026-10-19: the channel's acceptance revised

The implementation slices landed the whole design (the checklist's
EH-01..EH-15 Verified; the library surface EH-16 implemented through the real
frontend), and this amendment records the ownership decision that closes the
adoption (Option 2, decided 2026-10-19).

**What is verified.** The channel is implemented and verified end to end:
`checkThrows` is no longer an unimplemented foreign — the checked
`psrs:runtime-exception#checkThrows` contract is published, discharged at P9
into the inline `catch_all` try region, and executes under the pinned engine
through the real frontend: with a genuinely non-tail recursion, the guard is
interpreted when the Effect runs, the budget fires inside the guarded call,
and the catch delivers `true` (the checklist's direct-MIR and driver
fixtures, EH-09/EH-15/EH-16). The stable default profile enables `exceptions`
with the measured calibration (budget 262144; weights 32 + 16 × locals;
engine limit pinned `-W max-wasm-stack=524288`, wasmtime 48.0.1); the
budget's purpose is unchanged — it converts genuine non-tail exhaustion into
the compiler's exception, verified end to end (a depth-100000 genuinely
non-tail recursion under the guard exhausts and catches; exit 0).

**What is not reproduced, and stays a recorded failure row.** The target does
not reproduce upstream's TCO-classification aggressiveness. This backend's
mandated closure-tail reuse (`mark_tail` → `ReturnCallRef`; the verified
tail-call row CF-07) runs arity-mismatched mutual recursion in constant
stack, while upstream's TCO pass loopifies arity-matched named
mutual-recursion groups and leaves arity-mismatched closure-mediated cycles
growing the stack. `tests/upstream/passing/TCOMutRec.purs`'s four
`assertThrows` assertions therefore encode upstream's classification — its
own comment anticipates this ("the assertions can change") — and are
upstream-implementation-specific, not reproduced by this target: the guard
observes *no throw* (the guarded `ntcoN 100000` returns normally in constant
stack) and `assert'` takes its storage-trap failure. The case **stays a
failure row**, re-attributed from the P8 closure/linking row it held while
`checkThrows` was a missing foreign to a runtime-trap row. Reproducing the
assertions would need cross-function machinery the MIR does not have
(collapsing a mutual recursion group into one function's loop) — a normative
decision that would belong to a new recursion-classification topic, not to
this adoption.

**Consequence for the board.** The original expectation in the Consequences
above — TCOMutRec moving to passing and the board to 288/340 — is withdrawn;
the total stays 287/340 with the case's row re-attributed, and the profile
flip plus the board remeasure are recorded in D-04 with the measured
failure-set diff.
