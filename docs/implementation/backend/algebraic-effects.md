# Algebraic Effects: Proposed Acceptance Checklist

**Status:** Partial implementation evidence; all complete requirements below remain Unverified.
**Design:** [Library-owned algebraic effects and handlers](../../design/backend/fp/algebraic-effects-and-handlers.md)
(library model) and [Delimited control and resumable frames](../../design/backend/fp/delimited-control-and-resumable-frames.md)
(compiler mechanism).  
**Decision:** [DEC-19](../../decision/DEC-19-compiler-generated-resumable-frames.md)
selects compiler-generated resumable frames as the generic handler/interruption
mechanism.  
**Date:** 2026-10-11.

This checklist records the complete evidence required for implementation. Creating
it does not resume the withdrawn State migration or select native Effect/ST
representation. Compiler cleanup and runtime build-tool changes are separate
work. The `AC-*` obligations are **required**, not conditional: DEC-19 selected
the generic control mechanism, implemented as compiler-generated resumable
frames rather than target stack switching.

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| AE-01 | Existing PureScript syntax and type system express computation rows and handlers | Pinned upstream source; official `purs` accept/reject comparison on positive and negative library cases | Unverified |
| AE-02 | Handler removes its slot and preserves the residual row | Missing operation, wrong result type, residual row at pure boundary, and wrong-kind rejection cases | Unverified |
| AE-03 | Compiler does not recognize concrete effect/library identities | Code review and equivalent behavior for renamed ordinary operation modules; no State/Effect protocol hooks | Unverified |
| AE-04 | Construction is inert and interpretation preserves strict argument behavior | Operation count/order, strict factory arguments, ignored operation results, and repeat interpretation observations | Unverified |
| AE-05 | Pure State interpretation has correct value threading | Write/read result 42, nested handlers, independent labelled instances, and forwarded requests | Unverified |
| AE-06 | Deep handlers forward and resume correctly | Nested handler order, handled requests after resumption, and outer handling of clause requests | Unverified |
| AE-07 | Generic representation composes with handlers | Captures, rank-N interpreter arguments, polymorphic ADT/record fields, higher-order calls, and signature adapters | Unverified |
| AE-08 | Deep interpretation is stack safe | Actual Wasmtime execution of long bind chains, nested requests, and loops; compare observable results with pinned upstream oracle | Unverified |
| AE-09 | Generic optimization preserves observable behavior | Mandatory execution with ignored results, traps, callbacks, and pure newtype controls; no effect-specific optimizer exemptions | Unverified |
| AE-10 | Source fidelity is preserved | Independent package audit with pinned revisions; every target adaptation has a concrete representation reason | Unverified |
| AE-11 | Host interpretation uses a defined native IO boundary | Separate selected ABI design and actual host behavior evidence; a pure handler test is insufficient | Unverified |

The generic control mechanism selected by DEC-19 adds these required
obligations. They remain Unverified; library encoding alone does not satisfy
them:

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| AC-01 | Typed control API is compatible with the language decision | Reviewed DEC-03 amendment (DEC-19) and the complete `Control f a` carrier plus `return`/`bind`/`perform`/`run` contracts | Unverified |
| AC-02 | The selected representation can capture and resume | Actual target execution of resumable frames at supported boundaries, stack safety, and capability gating; no silent fallback | Unverified |
| AC-03 | Resumption multiplicity and lifetime are enforced | Double resume, re-entrant resume, cancellation, non-resuming clauses, and expired-boundary tests | Unverified |
| AC-04 | Captured values remain valid | GC during suspension, polymorphic capture recovery, foreign buffer/resource lifetime tests, and unsupported capture rejection | Unverified |
| AC-05 | Native execution preserves selected handler semantics | Deep/shallow boundary tests; distinguish one-shot rejection from permitted multi-shot behavior; suspension is not optimized away | Unverified |

The evidence sections below record focused runtime validation. No official
PureScript differential validation has been run for the control surface. Record
further evidence with compiler/package identities, commands, actual outputs, and any
skips. Do not infer topic completion from existing generic compiler tests.

## Prototype evidence (2026-10-11)

`crates/psrs-driver/tests/effect_representation.rs` compiles and executes a
self-contained prototype of the shared representation the design requires before
the native lowering: a rank-N eliminator whose method is rank-1 polymorphic, two
facades (`Effect`, `ST`) over the same computation field, an `unsafeCoerce` Global
conversion, and a pure interpreter. Mandatory `wasmtime` execution returns 42. Run
it with the locked standard library:

```sh
PSRS_STDLIB_ROOT=<locked stdlib> PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --test effect_representation
```

This is representation and interpretation evidence only. It does not establish
AE-04/AE-05/AE-06 (row-based handlers, deep/shallow, forwarding), AE-07 (captures,
rank-N interpreter arguments, higher-order calls, signature adapters), or any
`AC-*` obligation, because that prototype does not use compiler-generated resumable frames. No status above changes.


## Closed control executor evidence (2026-10-11)

The continuation machine is lowered in CC using layout-predeclared representations
and signatures. `Return`, `Perform`, and inert `Bind` nodes execute under a closed
`run`. A persistent continuation sequence is captured at requests; invoking a
resumption constructs internal `Resume` data rather than recursively executing
saved frames. MIR lowers the executor's self tail calls to loop back edges.
Ordinary conservative call effects retain interpretation with ignored results;
there is no control-specific optimizer exemption.

The baseline at `2026238f` passed the five existing `control_surface` tests.
`executes_a_performed_request` compiled but trapped with `unreachable` (Wasmtime
exit 134) rather than returning 42. The same case now returns 42.

`crates/psrs-driver/tests/control_surface.rs` covers the typed surface and the
return/request runtime paths. `crates/psrs-driver/tests/control_runtime.rs` adds
value-sensitive checks for left/right bind association, state/order, abandonment
of a trapping remainder, persistent multi-shot replay, replayed requests with
current handler state, nested delimiters, inert construction and strict factory
arguments, repeated interpretation, ignored-result handler traps, aggregate state
and captures, and callable answers. The stack-safety test executes 100,000-step
left-associated binds, right-associated binds, and request sequences under
Wasmtime. This tests machine traversal, not arbitrary recursion in user callbacks.

The public CC-to-MIR variant construction contract is also covered directly by
`variant_construction_lowers_at_abstract_and_common_representation_shapes`: both
field-bearing and nullary cases lower at abstract and concrete common variant
shapes. Construction first uses the concrete case type and then widens when the
CC destination names the common representation.

Run the focused evidence with the locked standard library and mandatory runtime:

```sh
PSRS_STDLIB_ROOT=<locked stdlib> PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --test control_surface --test control_runtime
```

This subsection records the closed-executor slice. Open elimination and GC
evidence are recorded below. One-shot APIs, expired-boundary rejection, complete
deep/shallow wrappers, foreign-resource lifetime checks, and official source
comparison remain unverified. The ordinary CC/MIR verifiers check call shapes,
fields, tags, and control flow; they trust Typed Core's answer-type relationship
and the synthesized machine's sequencing rules. They do not independently prove
all frame identity, lifetime, or multiplicity obligations in `AC-*`. No complete
requirement above changes status.


For the standalone request reproducer, the retained component
`/private/tmp/psrs-control-perform.wasm` executes with exit 42 under Wasmtime
49.0.2 (`3c8a3e79a`, 2026-10-02). Its SHA-256 is
`fc9f54c5934873af34beab1f3a8b745535a231cb7cbf8b3bfd8d94ab5bc9e192`.
The same source's baseline component trapped with exit 134.

The matching source diagnosis reports are `/private/tmp/psrs-control-before.json`
and `/private/tmp/psrs-control-after.json`, with retained Core/CC/MIR bundles.
Both compile successfully; compile diagnosis alone cannot observe the runtime
repair. The updated executor's retained MIR has loop jumps and no direct self
calls. Artifact canonical comparison and node lineage are unavailable in the
trace format, so this is manual MIR inspection plus actual execution evidence.


Validation of the closed-executor snapshot, using locked revision `1e01310`
and mandatory Wasmtime, completed on 2026-10-11:

- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Backend library tests: 423 passed, including the common variant regression.
- Control tests: 6 surface tests and 12 runtime tests passed, with no runtime skips.
- `cargo test --workspace`: failed at the first CLI integration target.
- `cargo test --workspace --no-fail-fast`: completed every target; 9 targets failed
  with 176 failing test names. A full baseline run from archived `2026238f` with
  identical stdlib/runtime settings has exactly the same failed targets and test
  names. No previously passing test became failing in this comparison.

| Failed target (baseline and updated) | Failed tests |
| --- | ---: |
| `psrs-cli --test cli` | 1 |
| `psrs-cli --test diagnose_trace` | 1 |
| `psrs-cli --test link` | 1 |
| `psrs-cli --test source_layout` | 1 |
| `psrs-driver --lib` | 163 |
| `psrs-driver --test data_representation_execution` | 1 |
| `psrs-driver --test pattern_matrix_execution` | 6 |
| `psrs-driver --test residual_constraints` | 1 |
| `psrs-driver --test visible_type_application` | 1 |

The CLI Prelude reproducer fails in both versions at P8 WIT linking because the
locked package still names the removed `psrs:effect` interface. Other baseline
failures are retained in the logs; the shared failure set is not a claim that
all 176 failures have one semantic cause. Workspace acceptance remains failing.

Baseline and final logs are `/private/tmp/psrs-control-baseline-all.log` and
`/private/tmp/psrs-control-workspace-verified.log`; their failure-set comparison
is `/private/tmp/psrs-control-workspace-comparison.json`. The final clippy log is
`/private/tmp/psrs-control-clippy-verified.log`. Shared-build-cache interference
was discovered during intermediate runs; those intermediate results were
excluded, and the final updated run rebuilt workspace packages after cleaning
their artifacts. These temporary files are local continuation evidence, not
portable repository artifacts. Official control differential cases and suite
scoreboards were not run; complete AE/AC statuses remain Unverified.


## Open control executor evidence (2026-10-11)

The open executor interprets inert `Handle` nodes and applies the library's
rank-2 Church-sum projection. Handled requests continue its loop; residual
requests stop that loop with `Bind(Perform(gop), forward)`. The generated forwarding
closure captures the persistent inner sequence and immutable handler state and
constructs another inert `Handle` when resumed. Both drivers share bind, return
and append reductions. `inject` is evaluated and retained; the original union
computation supplies the continuation, so no request reconstruction calls it.
The shared intrinsic summary now correctly classifies `handle` construction as
inert. Interpretation remains an ordinary conservative call.

`control_open_runtime.rs` has 17 mandatory-runtime tests covering:

- Handled/forwarded/handled ordering and saved state; three nested open/closed
  delimiters eliminating distinct ordinary request functors.
- Inner and outer abandonment, inner replay, outer replay with independent inner
  state, outer bind ordering, and residual requests created inside clauses.
- Inert construction, strict arguments, ignored interpretation traps, repeat
  interpretation, and equivalent behavior after renaming operation constructors.
- Aggregate state, request answers, results and captures through erasure.
- 100,000 iterations each containing a handled and a forwarded request.
- Deep reinstallation versus a library clause that forwards later requests to
  the outer handler. This is explicit library routing evidence; no dedicated
  shallow primitive or generic shallow wrapper has been implemented.
- A continuation held across 5,000 nested interpreted requests with a captured
  record. Wasmtime starts with a 65,536-byte GC heap; the test checks the completed
  collection log and the final captured result 42. This establishes collection
  of ordinary GC captures during suspension, not foreign resource lifetimes.

`control_surface.rs` adds rejection cases for an incompatible residual functor,
incompatible injection, and a projection specialized to one request answer.
The focused command is:

```sh
PSRS_STDLIB_ROOT=<locked stdlib> PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --test control_surface --test control_runtime --test control_open_runtime
```

All 38 focused tests passed (9 surface, 12 closed runtime, 17 open runtime), with
mandatory Wasmtime and no skips. The GC test retains the runtime collection log
at `$TMPDIR/psrs-control-open-gc-gc.log`. It requires a runtime exposing the pinned
Wasmtime logging/heap configuration. The tests do not use the `gc-zeal` debug
option, which ordinary Wasmtime builds can ignore.

The existing `AC-*` statuses remain Unverified: the monadic multi-shot API has no
explicit one-shot resumption or cancellation primitive, and generated resumptions
currently have no delimiter-liveness check. Ordinary verifiers trust the generated
machine's sequencing and checked source answer types. Direct-style selective
conversion, foreign capture checks and complete frame/lifetime verification remain
required. These obligations have not been removed from the normative design.


### Integrated validation of the open continuation

With the same locked package revision `1e01310` and mandatory Wasmtime:

- Formatting, `git diff --check`, and full workspace/all-target clippy passed.
- `cargo test --workspace --no-fail-fast` completed every target. The 9 failed
  targets and 176 failed test names exactly match the archived `2026238f`
  baseline; the comparison has no new failures and no recovered failures.
- Backend library tests: 423 passed. The 38 focused control tests also passed
  within this complete run, including the observed-GC test.

Logs: `/private/tmp/psrs-control-open-workspace-final.log`,
`/private/tmp/psrs-control-open-clippy.log`, and
`/private/tmp/psrs-control-open-focused-final.log`. Failure sets are recorded in
`/private/tmp/psrs-control-open-workspace-comparison.json`. An earlier workspace
run overlapped recompilation during tests and had stale-dependency rustdoc errors;
it is excluded from the evidence above. The final run completed before the next
build began. Workspace acceptance remains failing on the baseline failures.


The standalone mixed handled/residual request source is retained at
`/private/tmp/psrs-control-open.purs`. Matching before/after trace reports are
`/private/tmp/psrs-control-open-before.json` and
`/private/tmp/psrs-control-open-after.json`. Input/cohort/environment observations
match: the baseline stops at P8 closure conversion with unsupported open handling;
the updated source compiles with no diagnostics. The observed pass-status change
is `backend.cc.lower` from rejected to completed; canonical artifact comparison
remains unavailable.

The retained component `/private/tmp/psrs-control-open.wasm` executes with exit 42.
Its SHA-256 is `bb67426e18809c9d7142f0c768462af33760a42d62c98ad7f30150562622ce5c`.
Manual inspection of the retained MIR shows loop back edges for both executors.
The open executor retains one non-tail call to itself for nested `Handle` entry;
this matches the documented handler-depth bound and does not recurse once per
forwarded request. Wasmtime 49.0.2 executes the long forwarding test and the
collection test. Official differential cases and scoreboards were not run.


## One-shot ownership and delimiter lifetime evidence (2026-10-11)

The prior executor slice was committed as `1bcad5a7`. With that compiler, a handler
could store its continuation in an ordinary record, return from `run`, then call
and interpret the saved continuation in another `run`, returning 42. The owning
boundary is CC control lowering: the old captured sequence carried no liveness
evidence. `/private/tmp/psrs-control-lifetime-before.log` records execution of the
baseline case. The continuation now carries a delimiter token checked both on
invocation and interpretation; the same case traps with Wasmtime exit 134.

The compiler interface adds nominal `Resumption f a b`, `once`, inert `resume`, and
immediate `cancel`. Owners use a shared private status cell. Interpretation checks
Pending and changes it to Running without invoking a callback between these steps;
normal callback return changes it to Consumed. Cancellation changes only Pending
to Cancelled. Re-entrant callbacks observe Running, and all non-Pending uses trap.
The guarantee is atomic with respect to callbacks in the pinned single-threaded
guest profile. It is not an atomic protocol for shared-memory threads.
The mutable callback slot is cleared on consumption/cancellation, so a retained
spent owner does not root its callback indefinitely. This is ordinary CC/MIR GC
array storage; no target-native continuations or runtime frame interpreter were
added. Layout predeclares the new representations/signatures and stays immutable
during CC lowering. Scheme helpers were split into focused modules to retain the
500-line source limit.

The existing multi-shot handler API is preserved. Wrapping one unrestricted
function in two `once` calls creates independent owners; aliasing one owner shares
one consumption status. A one-shot library wrapper must keep the unrestricted
function private and expose only its owner. `resume` consumes an invocation when
interpreted, not when constructed. Its returned ordinary computation can still be
shared; this API does not impose a linear-use type system on `Control` values.

Executed evidence:

- `control_owned_surface.rs`: six checks for the nominal API, incompatible
  arguments/results/functors, cancellation of a bare function, and refusal to
  safely coerce the owner's answer parameter. The same-identity coercion control
  succeeds, so the rejection is not an unavailable import.
- `control_owned_runtime.rs`: fourteen mandatory runtime tests for one use,
  inert and abandoned resume nodes, aliases/double interpretation, repeated
  interpretation, cancellation, spent-owner cancellation, re-entrant resume and
  cancellation, independent owners, strict arguments, ignored callback traps,
  and collection while an owner retains an aggregate capture.
- `control_lifetime.rs`: six mandatory runtime tests for expired closed/open
  continuations, already-built Resume nodes, completion of an inner open
  delimiter while the outer remains live, invalidation of a suspended child
  after outer abandonment, and allowed re-entrant multi-shot interpretation
  while the original delimiter is still live.
- The previous 38 surface/closed/open control tests still pass, including
  outer replay and 100,000 forwarded requests. Replay creates a fresh activation
  with the checkpoint's original parent rather than a growing ancestry chain.

The owner GC case uses a 65,536-byte initial heap, requires observed completed
collections in Wasmtime's log, and then checks capture recovery with result 42.
The retained log is `$TMPDIR/psrs-control-owned-gc-gc.log`.

Run all focused evidence with the locked package and mandatory runtime:

```sh
PSRS_STDLIB_ROOT=<locked stdlib> PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --test control_surface --test control_runtime \
    --test control_open_runtime --test control_owned_surface \
    --test control_owned_runtime --test control_lifetime
```

Normal return closes delimiter tokens; a guest trap terminates the current command
path. The guest has no exception cleanup path under the selected profile. A host
that catches a guest trap and later re-enters the same retained state requires an
explicit invalidation/cleanup protocol; this is not proved by exit-134 tests.
Complete foreign borrowed-buffer/resource capture checks, host trap recovery,
frame-identity verification and official differentials remain unverified.
Ordinary library wrappers have separate executed evidence below. No complete AE/AC requirement is marked Verified by this slice.

## Checked primitive values and independent library wrappers (2026-10-11)

Ordinary source aliases such as `pure = Prim.Control.return` exposed a common P8
contract gap: compiler-interface primitive globals were omitted from mandatory
checked primitive-value elaboration. This is owned by Core primitive expansion
and the backend primitive-binding boundary, not by control-specific CC global
handling. The operation inventory now combines registry-owned intrinsic symbols
with validated foreign primitive bindings, even when the module has no foreign
bindings. Expansion preserves checked occurrence types and verifies the complete
candidate before publication. `control_primitive_values.rs` executes both aliases
(including rank-N `run`, `once`, `resume`, and scalar `intAdd`) and partial
applications, returning 42 in both cases.

Control callback/status cells also share the canonical array-layout owner with
ordinary array protocols. A direct layout test proves reuse; the existing
linked-module canonical-layout regression passes. Control layouts are declared
only when the linked type table contains the opaque carrier or owner, avoiding
unused synthetic layouts in pure modules.

The independent package worktree `/private/tmp/psrs-stdlib-control-20261011` is
based on the compiler's locked `1e01310` package and committed as `dda3ba3`.
The original stdlib working checkout and compiler lock are unchanged. The new
target-only `PSRS.Control` module has explicit provenance in `upstream-lock.json`;
no official module is edited. It supplies ordinary aliases and `runOnce` /
`handleOnce`, which expose one owner per request without leaking the unrestricted
continuation. Public package-runner cases `OneShot`, `MultiShot`, and `Open`
execute with exit 42; `DoubleResume` and `Cancelled` compile and execute with
exit 134 and an `unreachable` trap. Negative reports use reviewed byte-exact
runtime stderr files, as required by the existing runner. These do not establish
row compatibility, deep/shallow semantics, official `purescript-run` acceptance,
or host IO safety.

All five reports are `/private/tmp/psrs-control-library-<case>/run.json` and record
compiler/runtime/package fingerprints and artifact hashes. The package-owned
Node test suite passes all 11 tests. Its pinned source audit reports 41 packages,
231 modules: 170 identical, 36 modified, 25 target additions, and no absent upstream
modules or direct self-recursions. The audit is comparison evidence, not approval
of every existing target adaptation. Retained logs are
`/private/tmp/psrs-stdlib-control-node-tests-final.log` and
`/private/tmp/psrs-control-library-audit-final.log`.

Final compiler validation with the locked package and mandatory Wasmtime:

- `cargo fmt --all --check` and `git diff --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test --workspace --no-fail-fast`: 424 backend library tests and all
  66 control tests pass. The workspace still fails at the same 9 baseline
  targets, with 175 failing names versus 176 at archived `2026238f`. No new
  failure names are present; the recovered case is
  `tests::adts::erases_a_newtype_constructor_and_pattern_at_runtime`.

Final logs are `/private/tmp/psrs-control-owned-alias-workspace.log`,
`/private/tmp/psrs-control-owned-alias-clippy.log`, and the baseline comparison
`/private/tmp/psrs-control-owned-alias-comparison.json`. Earlier intermediate
runs that failed canonical-array reuse or suffered shared build-cache interference
are not final evidence. No official differential or scoreboard was run. Complete
AE/AC requirements retain their Unverified status.

## Main package cleanup and official type comparison (2026-10-11)

The stdlib main worktree previously contained committed `7852625` and uncommitted
State-protocol/WASI adaptations. At the user's request those old working changes
were archived in a named stash, and the main worktree switched to `control-library`
based on the locked package plus `dda3ba3`. The generic trap-runner improvement
was preserved separately. The old branch and stash retain recovery history; no
shared Git repository directory was deleted. The main package is now committed
at `28997b5` and clean. The compiler lock is still unchanged.

Official `purs` 0.15.16 found `UnknownExport` in the initial target wrapper:
imported type names cannot be listed as local exports. The wrapper now declares
local `Control` and `Resumption` type aliases. The retained before report is
`/private/tmp/psrs-control-official-types-before.stdout`.

The package-owned `conformance/control-types.mjs` compares five accepting and
four rejecting sources using both public compiler executables. Since official
`purs` does not implement `Prim.Control`, the official side explicitly uses an
opaque foreign-declaration shim with the same eight scheme shapes, local Unit
and scalar declarations, and a compiler-interface import remap. It emits CoreFn
only, with no fabricated foreign runtime. The wrapper bodies and aliases are
otherwise unchanged. Both compilers accept all five wrapper fixture sources and
reject wrong owner input, wrong result, a bare function passed to cancellation,
and a monomorphic clause with `TypesDoNotUnify`. The target rejection must come
from P5 typecheck; crashes, missing tools and unrelated diagnostics do not count.
The accepted report, including tool/source hashes and unchanged package
fingerprint, is `/private/tmp/psrs-control-official-types/types.json`.

This is focused ordinary-language type evidence. It does not establish official
control runtime behavior, pinned `purescript-run` compatibility, full rows or any
complete AE requirement. No official suite scoreboard was rerun.

All five public target runtime fixtures pass from the main package. Negative
cases now use `--expected-trap unreachable`: completed nonzero execution,
expected stdout and the terminal trap reason are required; raw stderr is still
retained. Backtrace offsets are no longer a golden. All 12 Node tool tests pass;
the pinned audit retains 41 packages / 231 modules (170 identical, 36 modified,
25 target additions), with no absent upstream modules or direct self-recursions.
Reports are `/private/tmp/psrs-control-main-<case>/run.json`; tool and audit logs
are `/private/tmp/psrs-stdlib-control-trap-node.log` and
`/private/tmp/psrs-control-main-audit.log`.

### Next owning contract: clause scope and deep/shallow reinstatement

`conformance/control/ClauseScope.purs` in stdlib is a retained failing acceptance
case. An inner clause performs a fresh request before invoking its continuation.
The scoped-handler requirement expects the outer handler to handle that request
and supply 42; the inner clause should be invoked once. Under `a31ba11e`, the
case compiles successfully but returns 0, because the inner fold handles both
requests. `/private/tmp/psrs-control-main-ClauseScope/run.json` records the
failure against expected exit 42. This case is excluded from the five passing
runtime fixtures and the nine type comparisons.

`cc/lower/control/open.rs::handled` calls the clause then recurs with the same
handler over its next computation. That implements the present state-threading
fold, but it does not carry the semantic evidence needed to distinguish clause
execution outside the delimiter from deep reinstatement of a resumed program.
A shallow continuation likewise cannot select removal of that handler with the
current fold surface. The next foundation must provide that distinction at one
generic typed delimiter boundary, keep clause and resumed-program domains
explicit, and retain lifetime/multiplicity evidence through CC. A hidden mutable
projection flag, concrete-effect dispatch exception or request suppression would
not satisfy the contract. AE-06 and AC-05 remain Unverified.

No Rust source changed in this continuation. Full Rust workspace tests/clippy
were not repeated; the prior `a31ba11e` evidence remains the compiler baseline.
The library tools, official type comparison, focused public runtime cases and
source audit were actually executed as recorded above.


### Typed elimination design correction (2026-10-11)

[DEC-20](../../decision/DEC-20-typed-control-elimination.md) and
[the scope contract](../../design/backend/fp/control-elimination-and-handler-scopes.md)
replace the draft five-argument open fold with a three-argument generalized
`handle`. No intrinsic was added and no compiler or production package source was
changed in this design continuation. The existing ID 96 remains selected.
`run` retains its closed state-fold contract. This is contract v2 design; compiler
`95982bea` and independent package `28997b5` still implement v1.

The ClauseScope failure is a missing typed API contract, not merely incorrect
CC dispatch: the old clause returns source `Control u a`, whereas the operator
returns target `Control g (s, a)`. Executing that clause as target data would be
unsound without a conversion. Revised clauses return the actual target domain;
a raw continuation remains in its source domain. Library code owns state,
projection, forwarding, deep reinstallation and shallow residual preservation.

#### Design validation actually performed

Artifacts are retained in `/private/tmp/psrs-control-contract-20261011/`.
Official `purs` 0.15.16 compiled `Contract.purs` to CoreFn using opaque foreign
carrier/primitive declarations, with no foreign runtime. The accepted source
contains the revised scheme, both complete wrapper bodies reproduced in the
scope design, a deep clause performing a target request, and typed deep/shallow
applications. `positive.json` records acceptance (shadowing warnings only).
Four separate negatives each report `TypesDoNotUnify`:

| File | Rejected relationship |
| --- | --- |
| `WrongClauseDomain.purs` | Source request performed in a target clause |
| `ShallowCannotRemove.purs` | Shallow source result assigned a reduced target effect domain |
| `WrongRawAnswer.purs` | Raw rank-2 continuation supplied an unrelated Boolean |
| `WrongTargetAnswer.purs` | Target answer assigned an unrelated result type |

Each file has a corresponding `.json` and `.stderr` report. These cases establish
ordinary type expressibility, not current target compiler acceptance of v2.
They are separate from the earlier nine package comparisons of the v1 interface.

`node model.mjs` executed eight rewrite-semantic assertions; `model.json` records
clause scope, deep reinstallation, shallow escape, residual forwarding, independent
multi-shot state, abandonment, return-clause scope and caller capture boundaries.
The expected clause-scope result is value 42 with one inner and one outer call;
deep handles both source requests internally, while shallow sends the second to
the outer handler. The model does not implement lifetime, GC, Wasm or a stack-safe
executor. No AE/AC requirement requiring guest execution becomes Verified from
these proofs. No official suite measurement changes.

#### Required coherent migration and evidence

| Obligation | Owner and required evidence | Status |
| --- | --- | --- |
| Revise checked scheme/arity, preserving intrinsic identity | Registry, primitive value elaboration, Core verification; direct calls, aliases, partial applications, malformed and old-signature rejection | Unverified |
| Preserve source/target contracts after erasure | CC layout and verification; predeclared SignatureIds and immutable representation table; cross-domain and mismatched finish-frame rejection | Unverified |
| Flat source/target context machine | CC lowering; inert construction, caller work outside capture, both clauses outside source dispatch, arbitrary nested handlers | Unverified |
| Typed library policies | Independent stdlib; port the shown wrappers and type negatives into package-owned conformance, compare with official `purs`, migrate ClauseScope and deep/shallow runtime cases | Unverified |
| Lifetime separate from dispatch | Mandatory Wasmtime; raw use during clause, expiry after target answer while parent stays live, expiry of already built nodes, abandoned child expiry | Unverified |
| Replay activation identities | Mandatory Wasmtime; replay of nested finish contexts, original checkpoint retained, replay-local captures expire, shared one-shot owners reject reuse | Unverified |
| Bound stack and inactive ancestor growth | Mandatory Wasmtime long deep/residual sequences; inspect frame/ancestor and allocation behavior; exercise non-tail multi-shot clauses separately | Unverified |
| Coherent artifact/package contract v2 | Explicit compatibility invalidation/rejection, revised independent package acceptance before lock update; no old/new overload or hidden extra intrinsic | Unverified |
| Complete adjacent obligations | Existing frame identity, GC roots, foreign borrowed-resource and caught-host-trap cleanup requirements remain in force | Unverified |

AE-06 and AC-05 remain Unverified. Their normative scope is not reduced to the
state fold. The earlier focused v1 runtime results remain historical evidence;
they do not certify the revised interface. Rust tests/clippy were not rerun for
this documentation-only correction; `git diff --check` is the relevant local gate.


## Control contract v2 implementation (2026-10-11)

Starting point: compiler `95982bea` on `feat/effect-handlers`, with the preceding
design correction uncommitted; independent stdlib `28997b5` on `control-library`,
initially clean. The default package selection rejected that main checkout because
it differed from locked `1e01310`. The same original mixed-forwarding test passed
with the explicit `PSRS_STDLIB_ROOT=/Users/biu/Projects/psrs-stdlib` development
package. Baseline log: `/private/tmp/psrs-control-v2-before.log`.

### Implemented ownership and machine contracts

- `ControlHandle` retains ID 96 and now has arity three and the DEC-20 scheme.
  There is no ninth intrinsic, legacy overload, compiler union projection or
  effect-name dispatch. The separate closed `run` still implements a state fold.
- Core rejects wrong saturated arity before indexing arguments, and checks all
  eight control carrier/callable relationships before erasure. Rank-2 clauses
  retain the shared request/raw answer variable and exact source/target domains.
  `tests::control` has four malformed/valid IR checks, including a forged source
  payload rejected at the common return constructor. Type relations use the
  existing Core checking owner; no private effect solver was added.
- CC layout predeclares computation, persistent queue, context and callback
  signatures. The single six-argument executor switches between root/source
  contexts. A source context saves its return clause, operation clause, caller
  queue, caller extent, caller context and owned extent. Both clauses execute
  with caller dispatch and a target finish marker, retaining the owned extent
  until their target answer completes.
- Raw resume remains inert and retains its original source queue/checkpoint.
  Replay flattens lifecycle templates with a heap traversal stack and rebuilds
  them from the outside inward with fresh activations. Ordinary callback values
  and one-shot owners remain shared. New activation ancestry uses the current
  dispatch extent; saved checkpoints retain their original owner. Helper edges
  are self tail calls lowered to loops, not recursive native sequence traversal.
- Tail handler entry normalizes leading `Append` nodes before inspecting finish
  markers. A leading compiler finish allows shared completion of the tail-returned
  handler without another lifetime ancestor or duplicate finish. User continuation
  heads keep independent extents. The intermediate long-forwarding run exposed
  missing queue normalization; it was stopped and superseded by the passing
  corrected run in `/private/tmp/psrs-control-v2-long-forward.log`.

Core verifies source typing. Existing CC/MIR verification checks generated
representation shapes and predeclared calls; full semantic frame/context identity
and malformed lifecycle-provenance verification remain Unverified. The long
runtime cases establish bounded native stack on the executed programs, not a
complete allocation/ancestor-depth measurement or proof for every control form.

### Requirement-to-test evidence

| Requirement slice | Actually executed evidence | Coverage |
| --- | --- | --- |
| Clause and return-clause scope; deep vs shallow | `control_scoped_runtime` fresh clause, return clause and distinct source-resumption cases | Focused Verified |
| Source/target answers and primitive values | Scoped alias/partial application changes Int source answer to Boolean target; `control_scoped_surface` rejects source requests in target clauses, wrong raw/target answers, old scheme and false shallow removal | Focused Verified |
| Replay, abandonment and caller capture | Scoped state replay/caller queue and abandonment; historical library-fold forwarding tests ported through an explicitly ordinary `foldOpen` helper | Focused Verified |
| Normal target lifetime | Scoped expiry of raw closures and prebuilt nodes while parent remains live | Focused Verified |
| Nested replay lifecycle | `control_scoped_replay` replays two nested finish contexts twice; new replay-local closures and prebuilt nodes expire after that branch | Focused Verified |
| Request/bind stack safety | Scoped deep and mixed forwarding cases execute 100000 requests; existing closed bind/request case also executes | Focused Verified; general allocation/depth bound unverified |
| Adjacent ownership, aggregate and GC behavior | Existing owned/open/closed control runtime targets; forced-GC evidence actually observes collection | Focused Verified |
| Package contract | Package test rejects explicit incompatible/null/string control versions; source rejects v1 handle scheme | Focused Verified |
| Complete verifier, foreign resources, caught traps, pinned reference library | No new execution establishes these contracts | Unverified |

The helper `foldOpen` in compiler tests is a user-defined state fold over v2
`handle`; it preserves historical fold behavior by explicitly converting its
source clause result. It is not the public scoped handler contract. Actual
shallow behavior is tested directly and through the independent public wrapper.
No compiler dispatch flag emulates it.

### Independent package and validation

The independent package now exposes revised `handle`/`handleOnce`, plus ordinary
`handleDeep`/`handleShallow`, and declares `control_protocol = 2`. Target wrapper
provenance/hash are updated; official PureScript module sources are unchanged.
The package-owned official comparison passes 17 cases: nine accepted sources and
eight `TypesDoNotUnify` negatives. It uses `purs` 0.15.16 and an opaque CoreFn-only
interface shim, not an official control runtime. Report:
`/private/tmp/psrs-control-v2-official/types.json`.

Nine public Wasmtime fixtures pass: `OneShot`, `MultiShot`, `Open`, `ClauseScope`,
`HandleOnce`, `Deep`, `Shallow`, `DoubleResume`, and `Cancelled`. The last two
require a completed semantic `unreachable` trap; the others require value 42,
including inner/outer counts where declared. Reports:
`/private/tmp/psrs-control-v2-public-<case>/run.json`.
Node tool tests: 12 pass. Pinned source audit: 41 packages, 231 modules (170
identical, 36 modified, 25 target additions), no missing upstream modules or
direct self-recursions. Logs: `/private/tmp/psrs-control-v2-node.log` and
`/private/tmp/psrs-control-v2-audit.log`.

Compiler validation with explicit development package and mandatory Wasmtime:

- Full workspace `--no-fail-fast` executes 78 control tests and all 424 backend
  library tests successfully. It still fails at the same nine targets and 175
  test names as `a31ba11e`; comparison has no added or recovered failure names.
  This compares failure names, not an assertion that every failure has one cause.
- Two additional nested replay tests execute successfully after the workspace
  run, bringing focused control coverage to 80 Rust tests. Four Core control IR
  tests and the package protocol rejection test pass in the workspace run.
- Formatting, diff checking and full workspace clippy pass. No new syntax,
  diagnostic code or official scoreboard measurement is claimed.

Logs: `/private/tmp/psrs-control-v2-workspace.log`,
`/private/tmp/psrs-control-v2-workspace-comparison.json`,
`/private/tmp/psrs-control-v2-nested-replay.log`, and
`/private/tmp/psrs-control-v2-clippy.log`.

Package protocol v2 and source fingerprinting prevent silently accepting an
explicit v1 package contract; old source schemes fail checking. The inspected
pipeline rebuilds Core/CC from source and exposes no persistent checked-IR cache
migration path. Existing standalone Wasm artifacts must be rebuilt; this change
does not claim a new runtime artifact-version rejection mechanism.
AE-06/AC-05 retain their full Unverified topic status, with the focused slices
above recorded separately. This implementation does not close the topic.


### Locked package integration

The package continuation is committed at
`5d523805c4e2096e68fab5a4f965f62b225ee2af`; the compiler lock now pins it with
`fnv1a64-v1:a1e5a2f8f4157041`. After the pin, all 80 control tests execute and pass
with `PSRS_STDLIB_ROOT` explicitly unset and mandatory Wasmtime, so ordinary
package selection is exercised rather than relying on the development override.
All five package-selection tests and four Core control IR tests also pass.
Logs: `/private/tmp/psrs-control-v2-locked.log`,
`/private/tmp/psrs-control-v2-package.log`, and `/private/tmp/psrs-control-v2-core.log`.
The full workspace failure-set comparison above used the explicit development
path to the same package content; it is not relabelled as a default-selection run.
