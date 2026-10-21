# Stage 1/2 review, 2026-10-21

**Scope:** Stage 1 Control/native exception bridge (`2e507111`, merged in
`58b2424c`), stage 2 typed abortive library (`psrs-stdlib` `a12e170`), and their
integration with the stage 3 native source-error channel. This is a code and
contract review, with focused counterexamples and executable evidence.
**Status:** Complete; both findings fixed and verified against the corrected compiler.

## Findings and corrections

### P2: protected self-call loopification violated native protection

`mir/lower/tail.rs` recognized self-calls before checking protected-region
membership. It could turn a valid protected recursive call into a back edge to
a Try block, after which P9 rejected its own output with InvalidCompilerIr.
More fundamentally, a loop back edge cannot generally replace a recursive
activation whose dynamic handler may receive a rethrow from its callee.
The former design rationale that a loop allocates no frame was insufficient.

A small direct CC reproducer has f(1) invoke f(0) inside generic Protect.
f(0) throws and its handler rethrows; f(1)'s older handler must return 42.
Before the fix, both the independent public compile_cc reproducer and the new
backend regression failed with `MIR back edge targets a try block`. The source
language frontend is not exercised by this counterexample, so no existing
PureScript program is claimed to reproduce it directly.

The common protection check now precedes self/direct/reference/closure tail
conversion. Protected self-calls remain ordinary native calls; unprotected
self-recursion still loopifies. The exception and tail-flow designs now state
the dynamic-handler invariant. The regression executes four observations:
tail-call capability enabled/disabled, MIR optimized/bypassed, all exit 42.
No verifier relaxation or new unsupported source subset is introduced.

Evidence: `/tmp/opencode/stage3/review/protected-self-call-before.log`,
`protected-recursion-before.log`, `protected-recursion-after.log`; repository
regression location `mir/tests/exception/protected_recursion.rs`.

### P3: the old checkThrows catch-path test had an ambiguous exit oracle

`mir/tests/exception/discharge.rs` expected exit 1 for a caught Boolean true.
The command entry also exits 1 for an uncaught native exception, so dropping
the catch could satisfy that assertion. This is a test-oracle weakness, not
proof that the existing discharge failed at runtime. Other source/bridge
observations already use distinct value-sensitive results.

The test now scales the Boolean to exit 42 and requires empty stdout/stderr.
An uncaught exception's exit 1 no longer satisfies it. The new stage 3 throw
probe uses the same distinct-success convention for all 36 observations.

### Documentation precision

The stage 2 acceptance table's 867 passing tests referred to the driver library
target, not the full workspace total. The detailed validation record already
said so; the summary row now uses the same scope. Historical stage 2 source
counts and package revision remain historical measurements, with the separate
stage 3 native API evidence linked explicitly. No corpus number was inferred
from focused tests.

## Reviewed ownership and integration

- `cc/lower/control/plan/run.rs`, `handle.rs`, `expand.rs` and boundary checks:
  runner-root close, Pending guard before acquisition, Running/Consumed stores,
  callback clearing before application, and exact original-token propagation.
  Exceptional close is emitted inline with private stores and no allocation or
  helper call; normal paths retain ordinary construction and lifecycle behavior.
- `cc/verify/ops/exception.rs` and token representation checks: fresh exception
  binder, body/handler scope separation and typed Yield/Propagate exits.
- `mir/lower/protect.rs`, shared `try_region`/`unwind`, CFG/constant propagation
  and tail marking: handler ownership, unknown-tag alternatives, outer-handler
  propagation, unwind reachability/dominance and preservation of dynamic guards.
- `wasm/lower/structure/region/try_unit.rs`: watermark saved before arming,
  restored before handler instructions, caught payload/token retained while
  the counter is restored, and ThrowRef preserving exception identity.
- `PSRS.Control.Except`: selected failures never invoke raw; residual requests
  forward once per encounter and reinstall the selected handler on response;
  recovery follows completed runExcept through an outer Bind; fromResult
  delays its leaf behind a callback, whereas rethrow consumes strict Either.
  Default-label aliases preserve complete IsSymbol/Row.Cons constraints.
- Source fidelity: unchanged pinned Run.Except/VariantF APIs and Functor
  instance; no compiler-recognized exception-effect module name; no implicit
  conversion from native exceptions/traps to arbitrary error payloads.

The pre-correction full workspace passed all 14 Control-bridge driver tests,
all eight typed-Except driver tests and 50 MIR exception tests. The corrected
workspace and fresh full stage 2 public suite have since completed, as recorded
below. The independent package remains clean at 618bede;
its typed-Except implementation is unchanged from a12e170.

## Known boundaries

Engine-trap recovery, foreign borrowed-resource cleanup, broader AC provenance
obligations, upstream recursion classification, transactional rollback and
asynchronous effects remain explicitly outside these completed consumers.
No new runtime defect was found in the stage 2 typed library during this review.
This statement does not establish full algebraic-effects or corpus-gate completion.

## Final review evidence

The corrected workspace passes 2247 tests with zero failed; five ignored corpus
scoreboards are separate from that claim. Format, Clippy and all doc-tests pass
serially. All 14 bridge driver tests and eight typed-Except driver tests pass.
The independent compile_cc reproducer now compiles and executes to exit 42; the
backend protected-recursion regression passes all four native-guard observations.
The legacy catch-path regression now proves caught success as 42.

The fresh stage 2 public suite records 26 accepted observations: 23 mandatory
executions, including five intended unreachable traps, plus three P5 type
rejections. A new official purs 0.15.16 Run/Except/State/Writer JS oracle run
produces seven observations whose Golden.purs is byte-identical to the goldens
used by that suite. See review/stage2-runtime/summary.json and
review/stage2-oracle/oracle.json under /tmp/opencode/stage3/. Stage 3's ten
public cases also pass on the corrected compiler. The final serial corpus
board is 287/340, 25 excluded, with unchanged agreeing/failing sets and every
agreeing exit zero; BigFunction remains explicitly without a compiler verdict
after resource-bounded allocation failure.

No further actionable defect was found within the reviewed stage 1/2 scope.
This conclusion retains the broader unverified AC obligations listed above; it
is not a proof of correctness for all compiler inputs or library APIs.
