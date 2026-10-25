# Recursive local values

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [CC IR](../../design/backend/fp/cc-ir.md),
[tail-call classification](../../design/backend/fp/control-flow-and-tail-calls.md#tail-call-classification)

**Measurement date:** 2026-10-25 (Asia/Shanghai).

This is a vertical slice of #101 under the Phase 5 backend epic #78. It does
not complete either issue. Successful recursive-value initialization and a
shared tail-continuation repair have execution evidence. Initialization demand
now raises a catchable source error with binding-module/read-line context;
failed initializers are never retried. The [RV-08 acceptance](recursive-initialization-errors.md)
records the baseline, implementation and actual Node/Wasmtime comparisons.

## Baseline and semantic ownership

The unchanged official `passing/Stream.purs` reaches checked Core and fails at
P8 closure conversion, bytes 535..595, with `recursive local binding is not a
function`. The first blocker belongs to local recursive binding conversion.
The pinned official compiler, purs 0.15.16, compiles the same program and Node
prints `Done` followed by a newline, exit 0.

The official generated JavaScript uses three-state memo getters and forces the
binding during initialization. Delaying an observable initializer until its
first use or expanding a source data binding into a function changes that
contract. Final same-input diagnosis recovers at `backend.cc.lower`; actual
Wasmtime 48.0.1 execution agrees with the official Stream output and exit.

## Construction and verifier boundary

P8 uses the existing dependency SCC owner. Components containing only syntactic
functions retain recursive closure conversion. Other cyclic components allocate
all typed state cells before constructing initializer getters. Each getter
returns the binding's declared runtime shape; a ready state stores that shape
without payload erasure. Getters share cells and capture external locals. The
containing computation forces every member in checked Core binding order
within its component before continuing to the body.

The local scope stores access mode and storage identity as one entry. Reading
a memo entry calls its getter; capturing it carries the getter itself. Ordinary
and recursive lambda restoration preserve this mode. The existing source-type
scope owns polymorphic use-site adaptation. Ready reads return the same cached
object. Only the uninitialized arm reconstructs getters for its initializer.

The emitted representation uses existing variants, mutable arrays and closures.
CC and MIR verification check ordinary storage, capture, signature and SSA
contracts. P8 owns the three-state transition and eager forcing protocol;
verifiers do not claim to derive that protocol from arbitrary array operations.
`ArraySet` mutates its existing source identity, as in private Control cells and
checked unsafe library storage. Pure source array updates clone first.

An initializing read executes `RaiseSourceError` on the shared native source
channel. The getter takes the current read's checked String message; capture
restoration retains binding-name/module context. The source catalogue validates
the range before line mapping. An initializer failure leaves Initializing set:
later reads raise their own messages without retrying work. Impossible state
tags retain a trap. This does not close the broad backend issue.

## Initialization continuation repair

A scalar probe exposed a defect in the common MIR tail pass. Four recursive
scalar bindings (`Int`, `Number`, `Boolean`, `String`) call a polymorphic recursive
selector that returns its initial argument. Official JS returns 42. Before the
repair, Wasmtime traps on a subsequent read: tail conversion skips the merge's
required write of the ready state, leaving the returned binding busy. The old
global returned-value set proved dataflow while ignoring continuation work.

The shared pass now checks the complete continuation from each candidate call.
Only instruction-free unconditional joins returning the identical result may
be discarded. Calls, writes, traps, branches and cycles preserve ordinary calls.
Block arguments carry result identity across empty joins. This same check owns
self loopification, direct tail calls, reference calls and closure calls.
Three MIR regressions cover an effectful join, an effectful continuation
returning a dominating value, and a later conditional trap. The scalar program
is the seventh maintained Wasmtime case and now returns 42.

## Requirement evidence

| ID | Requirement | Evidence | State |
| --- | --- | --- | --- |
| RV-01 | Unchanged official Stream compiles and executes | `comparison-v2-final.json`: same inputs, recovered at `backend.cc.lower`; `reference.json` and `executions-v2-final.json`: `Done`, exit 0 | Verified |
| RV-02 | Delayed reads and nested escaping closures retain access mode | `recursive_data_escapes_through_nested_closures` | Verified |
| RV-03 | Mixed function/value groups initialize eagerly once | `mixed_function_value_group_preserves_eager_once_only_initialization`; mutable counter and repeated reads; additional ordinary Ref observations | Verified |
| RV-04 | Cached mutable objects retain identity | `memoized_values_retain_shared_array_identity` | Verified |
| RV-05 | Demand during initialization fails, including an unused binding | `demand_during_initialization_raises_even_when_body_ignores_value`; accepted dynamic thunk shape, uncaught-source-error exit 1 | Verified |
| RV-06 | Callable aliases initialize before use | `callable_alias_is_initialized_before_its_first_call` | Verified |
| RV-07 | Recursive function capture restores memo access | `recursive_function_restores_a_captured_memo_access_mode` | Verified |
| RV-08 | Initialization-demand failure agrees with source exception recovery | Four actual Node/Wasmtime comparisons; five maintained driver tests, including distinct escaped reads/no retry and Control cleanup | Verified for message-only target Error; see RV-08 acceptance |
| RV-09 | Full workspace and official runtime measurement | Required-runtime workspace, fmt and Clippy pass; final initialization-error board remains 316/340 with no gained/lost agreement | Verified measurement; broader gate remains incomplete |
| RV-10 | Typed scalar initialization completes after recursive calls | `successful_scalar_memos_finish_after_recursive_initializer_calls`; three MIR continuation regressions | Verified |

The seven source execution cases are maintained in
`crates/psrs-driver/tests/recursive_values_execution.rs` and honor
`PSRS_REQUIRE_WASMTIME=1`. MIR regressions live in
`crates/psrs-backend/src/mir/lower/tail/tests.rs`. The backend initializer
regression checks eager calls and P9 acceptance. A pre-existing source-module
attribution test now uses a genuine incompatible scalar/array coercion in the
second input; it retains the diagnostic-origin assertion instead of expecting
the retired recursive-value rejection.

## Reference observations and limitations

`order-probe/comparison.json` observes two effectful mutually recursive Stream
initializers through ordinary `Effect.Ref`: pinned JS and Wasmtime both print
`12` followed by `42`, exit 0. The counter also observes one execution per
initializer. This single observation is not a general JS ordering guarantee.

The additional non-alphabetical `zed`/`alpha` variant in
`order-names-probe/comparison.json` records a difference: Node prints `21` then
`42`; the target prints `12` then `42`. Both initialize each value once and
produce the same pure result. The official reference implementation explicitly
says user declaration order is not preserved: its
`Language.PureScript.CoreFn.Laziness.applyLazinessTransform` builds an identifier
map and orders delay/force dependencies, adding lazy definitions when needed.
`Sugar.BindingGroups.toBindingGroup` separately orders immediate dependencies
and rejects immediate cycles. P8 follows its explicit checked Core sequence;
no agreement with the reference's initializer tie order is claimed for these
recursive `unsafePerformEffect` observations. Ordinary operand ordering and
required eager initialization remain unchanged. This limitation is retained
rather than introducing identifier-specific backend cases.

The accepted `demand-probe/Main.purs` keeps thunk forcing in an ordinary
top-level `applyThunk`. Official purs compiles it; Node's `Effect.Exception.try`
catches the initialization-demand error and prints its message. The baseline
Wasmtime artifact trapped at `unreachable`; the new comparison catches and
prints the identical message. The accepted unused-binding variant also fails during Node
initialization. An earlier direct `force`/`value` recursive group is rejected
by official purs as `CycleInDeclaration`; it is not a public runtime oracle.

The scalar comparison observes a source `Int` result: Node prints 42, while
the target entry returns process code 42. It is separate from the official
runtime board's successful zero exits.

## Validation and retained artifacts

The final RV-08 continuation passes 2,409 workspace tests and the full runtime
board remains 316/340. See the [current acceptance](recursive-initialization-errors.md)
for the 2211.729-second run and four source-error comparisons. The following
artifacts retain the earlier successful-initialization baseline.

Artifacts are under `/tmp/psrs-stream/`:

- `workspace-tests-v3-final.log`, `workspace-validation.json`: 78 targets,
  2,395 tests passed, zero failures, seven ignored, terminal exit 0. Command:
  `PSRS_REQUIRE_WASMTIME=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace`.
  The low-debug settings keep generated caches within this host's temporary
  filesystem. An earlier ordinary build exhausted that filesystem during
  linking; that attempt is not passing validation.
- `cargo fmt --all --check` passes. `clippy-v2-final.log` records successful
  `cargo clippy --workspace --all-targets -- -D warnings` with the same cache
  settings. All 631 backend units and the existing deep-tail/profile tests pass.
- `aff-regression-v2/aff.json`: all twelve library-owned Aff executions accepted,
  including nine public success comparisons with pinned JS, two target scheduler
  cases and one unhandled error case. The first attempt's sandbox `EPERM` is not
  semantic evidence; the complete local execution passed after approved rerun.
- `before.json`, its trace bundle, `after-v2-final.json` and
  `comparison-v2-final.json` retain the same-input Stream diagnosis.
  Preliminary relative-path `after.json` is unmatched and is not comparison
  evidence. `executions-v2-final.json` retains actual final Stream and scalar
  executions; `scalar-probe/regression-baseline.json` retains the earlier trap.
- Final frozen compiler: `psrs-v2-final`, SHA-256
  `321521255dbb46ea5888e6448e9c57d95baf38cb2a53265795e2a6e247b3af94`.
  Final frozen release harness: `suite-v2-frozen`, SHA-256
  `3cc5c3e2085e649518762515afe1c221ffa67ebbae26f34fc7266c41131ddecd`.
- `runtime-board-final/` and `runtime-audit-final.log`: the complete 365-root
  measurement is **316/340**, with 25 unchanged FFI exclusions and 74 support
  sources. `Stream` is the only recovery; no agreement is lost and all agreeing
  exits are zero. The run takes 2054.346 seconds with four workers, the same
  10-second guest limit, 524288-byte stack and compiler resource bounds.
  `terminal-status.json` records the observed terminal exit 0.
  `acceptance-audit.json` verifies all 439 corpus sources, 1,202 compiler input
  files, every per-root result, the frozen executable, and a byte-verified copy
  of 479 unchanged stdlib payload files. It also compares residual failure
  observations, which are unchanged. The lock remains
  `b817f3daf34247360216b48f10c14d8d14ccca3e` /
  `fnv1a64-v1:acd7fdd2f2a78577`.

The remaining 24 are eleven P5 type errors, three P5 kind errors, five P3
resolution errors, two P0 lexing errors, the unchanged `TCOMutRec` assertion
trap and `2288` guest timeout, and BigFunction's incomplete compilation. The
[last case](big-function-resource.md) aborts after 36.244 seconds with an observed allocation failure under
the compiler-only 8 GiB bound; it is neither a passing case nor a source
rejection. No remaining first blocker is P8 closure conversion.

A separate two-module `origin-probe` demonstrates a prerequisite for RV-08:
after P7 inlining, a P8 diagnostic reports Helper's bytes 153..165 as belonging
to Main, whose entire source is only 58 bytes. `origin-audit.json` retains both
source hashes and the actual out-of-bounds diagnostic. Source provenance must
survive linking and inlining before runtime initialization errors can claim
accurate source module/line information. Commit `3c14a07a` repairs general
provenance; the RV-08 implementation adds the validated source catalogue and
per-force message contract.

The first board was stopped at 236/365 roots on discovering the continuation
bug. `runtime-board-pre-scalar-fix/` has an explicit incomplete marker and is
not final gate evidence. No separate broad `PURESCRIPT_REPO` differential run
is claimed; the reference observations above actually executed purs and Node.
