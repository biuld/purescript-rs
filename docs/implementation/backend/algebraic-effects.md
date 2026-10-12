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

The evidence sections below record focused runtime validation. Official `purs`
type comparisons are recorded in the v2 section; complete pinned upstream-library
differential acceptance remains unverified. Record
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

## Nominal machine storage continuation (2026-10-11)

Starting point: compiler `bdaf0cfc`, a clean worktree, and the default locked
stdlib `5d523805c4e2096e68fab5a4f965f62b225ee2af`. The scoped runtime, nested
replay and lifetime targets pass before the change; their baseline log is
`/private/tmp/psrs-control-typed-frames-before.log`.

CC layout now stores computation links, queue tails, finish extents, caller
contexts and ownership cells with explicit representation handles. Stored
callbacks name the predeclared unary signature. Primitive and synthesized
constructors consult this same immutable schema; projections declare its field
shape before a separate erasure. Existing erased carrier/helper boundaries use
explicit reference casts. They recover existing objects without invoking source
array payload conversion, which would adapt/copy a shared integer status cell.
The target planner retains recursive nominal fields in MIR struct types.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Reject wrong or erased machine storage fields | `control_frames_reject_erased_or_wrong_machine_fields_in_cc`: resume queue, handle body, append queue, finish extent, caller context, owner status and boundary parent | Focused Verified |
| Preserve typed projection before erasure | `control_frame_projection_cannot_silently_erase_checkpoint_identity` rejects an erased declaration for a finish extent projection | Focused Verified |
| Preserve field contracts into MIR | `mir_preserves_control_frame_fields_and_rejects_cross_wired_storage` lowers valid recursive frames, then swaps fields in resume, finish, context, owner and boundary structs; MIR rejects wrong argument types | Focused Verified |
| Preserve execution and shared ownership | All 80 control tests pass with mandatory Wasmtime and default package selection, including reentrant live raw resume, nested replay, expiry, cancellation, GC and long requests | Focused Verified |
| Prove semantic source/target identity and checkpoint provenance | Same-shaped extents/domains can still be exchanged without this storage check proving their ownership; erased helper calls and reference casts remain trusted producer boundaries | Unverified |

Trap assertions now require an actual guest trap and reject cast failures, so
incorrect machine representations cannot satisfy expected control rejection.
Ordinary strictness/ignored-result fixtures retain their divide-by-zero traps.
No package source, control signature, intrinsic identity or gate measurement
changes in this continuation. Full AE/AC acceptance remains Unverified.

Validation logs: `/private/tmp/psrs-control-typed-frames-ir.log` (three new
malformed-IR tests), `/private/tmp/psrs-control-typed-frames-control.log` (80 control
tests), and `/private/tmp/psrs-control-typed-frames-clippy.log` (workspace clippy).
Formatting and diff checking pass. `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
--no-fail-fast` executes all 427 backend tests and the control targets successfully.
The nine pre-existing non-doc targets still fail at exactly the same 175 test
names as the v2 baseline, with no added or recovered names. Two additional
doctest targets report `E0463` (missing dependency crate) in that run while other
validation commands rebuild shared artifacts; a subsequent serial
`cargo test -p psrs-backend -p psrs-driver --doc` passes both targets (zero doc
examples). This rerun resolves the observed operational failures, without
relabeling the initial full workspace run as passing.
Logs: `/private/tmp/psrs-control-typed-frames-workspace.log`,
`/private/tmp/psrs-control-typed-frames-comparison.json`, and
`/private/tmp/psrs-control-typed-frames-doctests.log`. No official scoreboard or
upstream type comparison is rerun for this internal storage change; the prior
package type/source evidence is retained rather than remeasured here.

## Typed machine helpers and captures (2026-10-12)

Starting point: compiler `5c78fa78`, a clean worktree and unchanged default locked
stdlib. The lifetime, nested replay and primitive-value targets pass at this
checkpoint and after migration. Baseline:
`/private/tmp/psrs-control-typed-calls-before.log`; focused result:
`/private/tmp/psrs-control-typed-calls-focused.log`.

Previously, stored fields were nominal but synthesized helpers immediately
erased projections and passed every argument/result as erased data. The raw
resume closure also declared its queue and checkpoint captures as erased. This
continuation removes those unnecessary erasures. Layout predeclares the complete
helper signatures; registration, call construction and function result checking
consume them, without allocating late SignatureIds or duplicating the registry.

| Helper | Parameters | Result |
| --- | --- | --- |
| Executor | Erased state, unary handler, computation, queue, active extent, dispatch context | Erased user state/answer pair |
| Clone sequence | Queue, traversal stack, reversed queue, active extent, parent extent | Replay result |
| Rebuild sequence | Reversed queue, result queue, active extent, parent extent | Replay result |
| Start boundary | None | Boundary |
| Check/close boundary | Boundary | Boundary |
| Once | Unary callback | Owner |
| Cancel | Owner | Owner |

Replay result is a dedicated product with nominal queue and active-extent
fields; it cannot be recovered as an ordinary user state/answer pair. Machine
constructors, projections, status cells and raw resume captures retain their
nominal shapes. User callback application alone adapts its arguments and result
through the existing erased curry protocol. Clearing a spent owner's callback
uses an explicit reference erasure, retaining the shared status-cell identity.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Reject erased and wrong helper arguments | `control_helper_calls_reject_erased_and_cross_wired_machine_arguments`: executor, clone/rebuild, lifetime and once/cancel | Focused Verified |
| Distinguish replay result from user pair | `control_replay_result_cannot_be_declared_as_an_erased_user_pair` rejects an erased call-result declaration | Focused Verified |
| Preserve queue/checkpoint capture roles | `raw_resumption_capture_preserves_queue_and_checkpoint_shapes` accepts typed capture and rejects swapped/duplicate capture slots through common closure verification | Focused Verified |
| Preserve helper roles into target calls | `mir_rejects_machine_call_arguments_with_swapped_queue_and_extent` lowers valid CC calls into MIR tail calls, then rejects wrong argument types | Focused Verified |
| Require production lowering to retain these contracts | `control_machine_preserves_nominal_capture_and_replay_contracts` lowers checked source to CC and inspects actual executor parameters, raw captures, replay products and lifetime helpers | Focused Verified |
| Prove source/target domain relation and exact checkpoint owner | Different checkpoints still share `Boundary`; the source/target request and answer relation is checked in Core and not retained as a semantic domain contract in CC/MIR | Unverified |

No source control scheme, package revision, intrinsic identity, effect inference
or scoreboard measurement changes. This supplies nominal call/capture contracts,
not the complete semantic provenance proof. The remaining verifier must retain
the source/target contract identities and distinguish captured checkpoint owners,
replay activations and completion boundaries even when their physical shapes
match. Complete AE/AC requirements stay Unverified.

The four new backend malformed-IR tests pass, and the source-to-CC test passes.
Logs: `/private/tmp/psrs-control-typed-calls-ir.log` and
`/private/tmp/psrs-control-typed-calls-source-ir.log`.

Final validation uses default locked package selection and Wasmtime 49.0.2
(`3c8a3e79a`), with no development package override. Formatting, diff checking
and `cargo clippy --workspace --all-targets -- -D warnings` pass.
`PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast` executes all
431 backend tests, all 80 control tests and the new source-to-CC test successfully.
The full run still fails at the same nine non-doc targets and 175 test names;
failure-name comparison with the previous storage checkpoint adds or recovers
no names. Doctests complete without the prior dependency-artifact errors.
The final workspace run is serial with respect to other Cargo commands.

Logs: `/private/tmp/psrs-control-typed-calls-workspace.log`,
`/private/tmp/psrs-control-typed-calls-comparison.json`, and
`/private/tmp/psrs-control-typed-calls-clippy.log`. Upstream type comparison,
package conformance and official scoreboards are not rerun for this internal
machine change; their previous evidence is retained. No gate count is updated.

## MIR closure capture contracts (2026-10-12)

Starting point: clean compiler `d2125f5c`, default locked package selection.
Baseline: `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test
control_scoped_replay --test control_lifetime` passes all eight runtime tests
(`/private/tmp/psrs-control-mir-captures-before.log`). The owning boundary is
MIR closure construction: the previous verifier checked eqref representability
without relating capture slots to the locally defined target's projections.

Common MIR module verification now derives slot types from surviving projections
of the target receiver, following receiver copies and reference casts. Every
local `ClosureNew` must supply compatible values at those indices. Conflicting
reads of one receiver slot are invalid. After optimization, projection contracts
may be sparse; the verifier neither renumbers slots nor requires unused stored
captures to disappear. Imported targets and projections from unrelated closure
values retain their trusted boundary. No synthesized function name or control
intrinsic is used to interpret the contract.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Preserve raw queue/checkpoint roles through MIR capture storage | `mir_raw_resumption_rejects_swapped_duplicate_and_missing_captures` lowers valid CC, then mutates MIR slots | Focused Verified |
| Check surviving slots after dead projection elimination | `mir_capture_contract_survives_sparse_projections_after_optimization` accepts a live slot 1 without slot 0, then rejects the wrong stored value | Focused Verified |
| Reject contradictory receiver reads | `mir_capture_contract_rejects_inconsistent_reads_of_one_slot` | Focused Verified |
| Preserve receiver identity across ordinary aliases | `mir_capture_contract_tracks_receiver_copies_and_reference_casts` accepts both aliases and rejects wrong captures through each | Focused Verified |
| Prove exact checkpoint owner and source/target domains | Nominally identical boundaries still have no semantic provenance proof | Unverified |

This extends nominal capture-role verification to MIR; complete AE/AC identity,
foreign-resource, cleanup and allocation obligations remain outstanding.

Validation uses default locked package selection with mandatory Wasmtime 49.0.2
(`3c8a3e79a`). `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`
passes all 435 backend tests and all 80 control tests. The workspace still fails
at the same nine targets and 175 test names as `d2125f5c`; comparison adds or
recovers no names. After preserving the owning module in the new diagnostics,
the backend suite is rerun on the final source. Formatting, diff checking and
`cargo clippy --workspace --all-targets -- -D warnings` pass.

Logs: `/private/tmp/psrs-control-mir-captures-workspace.log`,
`/private/tmp/psrs-control-mir-captures-comparison.json`,
`/private/tmp/psrs-control-mir-captures-backend.log`, and
`/private/tmp/psrs-control-mir-captures-clippy.log`. Pinned upstream-library
conformance, explicit official type comparison and scoreboards are not rerun;
no gate measurement or topic-completion status changes.

## Tail-handler ancestry and GC cost evidence (2026-10-12)

Starting point: clean compiler `148e706b`, with the same locked stdlib revision
`5d523805c4e2096e68fab5a4f965f62b225ee2af`. The existing long-request tests only
observed native stack safety. They did not inspect extent ancestry or retained
heap. This slice supplies those measurements without adding a control intrinsic.

`tests::control_costs` lowers checked source to CC and inserts an ordinary
direct-call probe after every boundary allocation in the generated machine,
including nested branches and replay helpers. The probe walks the actual parent
chain and traps when its budget is exceeded. `compile_cc(BackendInput)` rechecks
CC and runs the ordinary MIR, optimization, linking and Wasm emission. A malformed
instrumented input is rejected before emission. The same layouts, source control
schemes, capture slots and lifecycle transitions are used by production lowering.
The instrumentation adds no heap objects to the machine.

`control_tail_handlers_bound_extent_ancestry_and_post_gc_live_heap` executes
deep reinstatement and mixed handled/forwarded loops at three scales. Each run
asserts the final value 42 and the exact source/outer request counts. Every new
extent has at most two parent edges. With a 65,536-byte initial GC heap and actual
copying collections observed under Wasmtime 49.0.2 (`3c8a3e79a`), the results are:

| Workload | Steps | Collections | Maximum post-GC live bytes | Component fingerprint (`fnv1a64-v1`) |
| --- | ---: | ---: | ---: | --- |
| Deep | 1,000 | 94 | 1,408 | `3dd8da22d97c8e7d` |
| Deep | 10,000 | 941 | 1,408 | `88b822c5f0cf24ef` |
| Deep | 100,000 | 9,411 | 1,472 | `9e9c3d60547570a9` |
| Mixed forwarding | 1,000 | 393 | 1,440 | `3d7123bbbf850041` |
| Mixed forwarding | 10,000 | 3,934 | 1,440 | `65b8375305188f41` |
| Mixed forwarding | 100,000 | 39,344 | 1,440 | `d1bea8d6df3f2cd0` |

These are measurements of instrumented workloads, not a general bound on all
programs. Collections measure churn at a fixed initial heap size; they do not
measure exact total allocated bytes or timing without the probe/logging overhead.
Post-GC live bytes measure retained objects at collection points. Together with
the ancestry guard, they reject unbounded inactive parent retention in these
tail-handler loops. The test uses a 16,384-byte live-heap ceiling to allow ordinary
layout and root-set variation while still detecting retained linear growth.

`control_ancestry_probe_observes_independent_non_tail_extents` executes eight
nested handles separated by user continuations. A budget of eight returns 42;
a budget of seven produces a guest trap without a cast failure. This establishes
that the probe observes actual ancestry and that the machine preserves distinct
non-tail scopes. Existing non-tail multi-shot and branch-expiry execution remains
separate semantic evidence.

Reproduce the focused measurement with:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib control_costs -- --nocapture
```

Source, component, GC trace and JSON reports are retained under the OS temporary
directory as `psrs-control-cost-{deep,forwarded}-{steps}-2.{purs,wasm,gc.log,json}`;
the nested controls use `psrs-control-cost-nested-8-{8,7}`. JSON records the
component fingerprint, observed exit, configured heap, ancestry guard and GC
observations. Focused log: `/private/tmp/psrs-control-costs-focused.log`.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Bound inactive ancestry for long tail deep/forwarded loops | Actual allocation-site parent walks at all three scales, budget two | Focused Verified |
| Measure retained heap and churn separately from native stack | Required copying collections, live-byte ceiling and retained scale reports | Focused Verified |
| Preserve independent non-tail scopes and prove probe execution | Eight-versus-seven ancestry-budget control | Focused Verified |
| Keep instrumented execution inside stage contracts | `control_cost_pipeline_rejects_a_malformed_instrumented_input`; common CC/MIR verifiers and target pipeline | Focused Verified |
| Complete semantic identities, foreign lifetime, caught traps and pinned library acceptance | This measurement does not discharge those obligations | Unverified |

Final validation: `cargo fmt --all --check`, diff checking and
`cargo clippy --workspace --all-targets -- -D warnings` pass.
`PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast` passes all 435
backend tests, all 80 control integration tests and the three new cost tests.
It still fails at the same nine targets and 175 test names as the preceding
checkpoint; failure-name comparison adds or recovers no names. Logs:
`/private/tmp/psrs-control-costs-workspace.log`,
`/private/tmp/psrs-control-costs-comparison.json`, and
`/private/tmp/psrs-control-costs-clippy.log`. Explicit upstream type comparison,
package conformance and scoreboards are not rerun for this internal cost slice.
No gate measurement or complete AE/AC status changes.

### Completion audit after the cost slice

The objective remains complete delimited control, followed by native Effect/ST.
The governing designs keep those owners separate. Before advancing to the native
facades, remaining mechanism acceptance must cover:

- Retained checked source/target domain relationships and exact checkpoint,
  replay-activation and finish-boundary provenance, with malformed-IR rejection.
- Foreign-buffer/resource capture validity and explicit unsupported-boundary
  rejection; ordinary GC captures do not supply this evidence.
- A defined caught-host-trap recovery/cleanup boundary and actual recovery
  execution; terminal guest traps alone do not supply it.
- Complete pinned upstream library/source and execution comparisons for the
  generic handler model, including the remaining row/representation interactions.
- Cost evidence for the remaining required control forms and an explicit account
  of total allocation where the selected acceptance contract requires it.

AE-11 native host interpretation and the official Effect/ST facades belong to the
subsequent native topic. This dependency ordering does not change their required
acceptance or narrow any AC requirement above.


### Checked source contracts through physical lowering (2026-10-12)

This continuation starts from compiler `2e88d656`. Core's existing control checks
now have one shared API, `checked_control_contract`, returning a sealed proof.
Core verification still traverses the operand expressions and supplies its scoped
type view. P8 checks the same operation contract, quotes its complete result,
arguments and source/target domains, and plans the factory representation/case or
helper signature before the CC representation table becomes immutable.

The quotations retain nested quantifiers, binder identities, stable constructors
and open rows. Canonical type identities are allocated using Core's existing
rigid equality; the backend does not infer effect rows or add a private relation.
`ControlEvidence` is an explicit erasure-boundary witness. CC checks the actual
factory's ordered operands, allowing the checked representation conversions, and
requires inert frame results to be the proven construction. Known control
producers in the same CC assignment sequence must supply the expected canonical source type. MIR retains the source
proof, validates references/arity/result shape and checks known producer domains
through copies/casts after optimization. The final encoder emits only a value
copy. No new source intrinsic, heap frame or host import is added.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Share the authoritative checked source contract | Core `checked_control_proof_retains_both_domains_and_the_full_clause_type`; existing malformed carrier/callable tests | Focused Verified |
| Retain distinct request/answer domains and rank-two clauses through P10 | `checked_domains_and_rank_two_clause_survive_physical_lowering`, actual checked F/Int to G/Boolean source | Focused Verified |
| Reject missing contract references | `cc_rejects_a_dangling_control_contract`, `mir_rejects_a_dangling_control_contract_after_optimization` | Focused Verified |
| Connect proven operands to actual frame construction | `cc_rejects_a_frame_that_does_not_save_the_proven_source_body`; legal cast shapes with wrong saved operand | Focused Verified |
| Reject a known target-domain body as a source computation despite compatible physical storage | `cc_rejects_a_same_shape_body_from_the_target_domain`, `mir_rejects_an_operand_from_the_other_checked_domain` | Focused Verified |
| Establish complete semantic provenance | Parameters, ordinary call results, imports, aggregate projections, CC parent-sequence values and joins without local witnesses remain trusted; exact checkpoint/replay/finish identity is absent | Unverified |

MIR trusts P9's translation of the checked factory connection; this unit does not
reconstruct that proof from physical struct instructions or establish helper
implementation identity from a matching signature. The complete AE/AC rows stay
Unverified. Foreign resource/buffer capture, caught-host-trap cleanup, full pinned
library acceptance and the remaining cost obligations also remain open.

Focused validation passes 435 backend and 95 Core tests, 21 surface/scoped tests,
and 11 filtered driver tests (including the six new source/provenance tests and
the existing three ancestry/GC cost tests). Logs:
`/private/tmp/psrs-control-contracts-backend.log`,
`/private/tmp/psrs-control-contracts-focused.log`, and
`/private/tmp/psrs-control-contracts-driver.log`.

The six long-loop cost measurements are rerun with the existing scales, 65,536-byte
initial heap, ancestry bound two and live-heap ceiling 16,384 bytes. Collection
counts and maximum post-GC live bytes remain exactly those measured in the prior
cost slice. Component identities change because live witnesses encode value
copies; this continuation's retained reports record:

| Workload | Steps | Component fingerprint (`fnv1a64-v1`) |
| --- | ---: | --- |
| Deep | 1,000 | `5ed1de202d259abd` |
| Deep | 10,000 | `4b2c2d30ffa39ce5` |
| Deep | 100,000 | `c27e72b4855c6211` |
| Mixed forwarding | 1,000 | `4cb540e330538aed` |
| Mixed forwarding | 10,000 | `2df8c32f11ae2d73` |
| Mixed forwarding | 100,000 | `c4a675dd4d73a60c` |

Reports use the same OS temporary paths recorded in the cost slice. These
measurements still do not supply a general allocation bound or exact total
allocated bytes. The unchanged semantic and lifetime obligations must be
completed before starting native Effect/ST.

Final integrated validation: `cargo fmt --all --check`, `git diff --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.
`PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast` executes every
workspace target. All 435 backend tests, 95 Core tests, 80 control integration
tests, six new driver source/provenance tests and three existing cost tests pass.
The nine failing targets and 175 failed test names are exactly the same as at
`2e88d656`; the comparison adds or recovers no names. This is a failure-name
comparison, not a new classification of each baseline cause. Logs and comparison:
`/private/tmp/psrs-control-contracts-workspace.log`,
`/private/tmp/psrs-control-contracts-comparison.json`, and
`/private/tmp/psrs-control-contracts-clippy.log`. Explicit upstream type comparison,
package conformance and suite scoreboards are not rerun for this internal stage
contract unit; no measured gate number changes.


### Semantic helper callee bindings (2026-10-12)

This continuation starts from `57e94257`. Check and close have the same callable
signature but different lifetime meanings. Previously, exchanging their callee
symbols could pass all physical call checks. P8 now predeclares an immutable
helper binding for each executor/lifecycle/replay/owner role before any function
body is lowered. The binding contains the concrete helper symbol and its
predeclared signature. The source contract planner retains the first owning
run/once/cancel declaration's module; generated diagnostics do not acquire an unrelated
entry module merely because helper allocation moved earlier.

Machine direct calls carry their `ControlHelperId`; ordinary calls carry no
machine contract. CC checks the exact callee binding and rejects missing or
invalid contracts. Run/once/cancel source witnesses additionally require their
expected helper role. P9 checks call identity before local/import conversion and
before self loopification. Ordinary MIR calls and non-self tail calls retain and
verify it. P10's common inliner may discharge a verified call by substituting that
resolved callee's body; common self loopification discharges the verified self
binding. This adds no source intrinsic, runtime frame or optimization exemption.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Distinguish semantic helpers with equal signatures | `cc_rejects_exchanging_check_and_close_despite_equal_signatures`, `optimized_mir_rejects_exchanging_check_and_close` | Focused Verified |
| Reject loss of a required machine call contract | `cc_rejects_removing_a_machine_call_contract`, `mir_rejects_missing_identity_on_a_tail_helper_call` | Focused Verified |
| Validate before P9 can discharge a call into a loop/import lowering | `p9_rejects_a_wrong_helper_before_lowering_or_loopifying_calls`, actual malformed CC through the public P9 entry | Focused Verified |
| Preserve helper identity across non-self tail conversion | `mir_tail_calls_retain_the_predeclared_helper_identity`, actual checked-source optimized MIR | Focused Verified |
| Execute the persistent driver without tail-call capability | `control_helper_contracts_execute_long_requests_without_tail_call_capability`: compile with `tail_call: false`, assert no MIR tail-call terminators, run Wasmtime with `-W tail-call=n`, count exactly 100,000 requests and return 42 | Focused Verified |
| Verify complete checkpoint, activation and finish provenance | The bound callee may still receive a wrong same-shaped boundary; helper-body lifecycle correctness is not proved by the binding alone | Unverified |

Focused log: `/private/tmp/psrs-control-helpers-focused.log` (seven tests, mandatory
Wasmtime). Earlier source/provenance and ancestry/GC probes also pass in the
filtered control driver run. Complete AE/AC status remains Unverified; source and
checkpoint argument relations, foreign capture, caught-trap cleanup, full pinned
library comparison and remaining general allocation obligations remain open.


Final validation: `cargo fmt --all --check`, `git diff --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass. Mandatory Wasmtime
workspace execution passes all 435 backend tests, 95 Core tests, 80 control
integration tests, the seven new helper/capability tests and the existing
source/provenance and three cost tests. The nine failed targets and 175 failed
test names exactly match `57e94257`; no names are added or recovered. This compares
failure names, not every baseline cause. Logs:
`/private/tmp/psrs-control-helpers-workspace.log`,
`/private/tmp/psrs-control-helpers-comparison.json`, and
`/private/tmp/psrs-control-helpers-clippy.log`. The six ancestry/GC workloads are
remeasured; component fingerprints, collection counts and maximum post-GC live
bytes exactly match the preceding source-contract slice. Explicit upstream type
comparison, package conformance and scoreboards are not rerun for this internal
call-contract unit, and no measured gate number changes.


### Finish consumer ownership (2026-10-12)

This continuation starts from `df46f2d2`. Same-shaped boundaries could still be
exchanged while keeping the correct close helper identity. A shared relation now
checks actual finish consumption: the closed extent, restored queue and restored
parent must be projections 0/1/2 of one immutable finish record. CC reads actual
variant projections and execution calls; MIR reads actual struct projections and
execution calls/tail calls/self-loop jumps. The schema comes from the predeclared
sequence helper signature and physical layout, not function names or a new
source intrinsic. Both directions are checked: a close needs the matching next
step, and a step consuming a finish queue needs a preceding close.

CC reference identity operations have one shared owner. Identity conversion,
reference erasure/recovery and reference casts preserve an existing object's
identity; mappings/adapters that construct another object do not. The finish
verifier indexes definitions across nested CC sequences, while the existing
source-contract consumer uses the same transparent-operation rules. MIR follows
copies and casts, including the self-loop parameters produced by P9. No runtime
instruction or representation changes are introduced by this verification unit.
Incomplete nonempty helper registries and missing sequence schemas are errors.

| Requirement slice | Evidence | Status |
| --- | --- | --- |
| Close the finish extent rather than its parent | `cc_rejects_closing_the_parent_instead_of_the_finish_extent`, `optimized_mir_rejects_closing_a_same_shaped_parent` | Focused Verified |
| Restore the proper parent | `cc_rejects_restoring_the_closed_extent_instead_of_its_parent`, `optimized_mir_rejects_the_wrong_restored_parent` | Focused Verified |
| Require all restored fields to come from the same record | `cc_rejects_a_parent_projected_from_another_finish_frame`, `optimized_mir_rejects_a_parent_from_a_different_finish_frame` | Focused Verified |
| Require completion rather than a compatible check or omitted call | `cc_rejects_a_finish_disguised_as_a_valid_lifetime_check`, `optimized_mir_rejects_a_finish_without_a_close` | Focused Verified |
| Accept identity-preserving transformations | `checked_reference_identity_aliases_preserve_finish_ownership`, real CC through the checked artifact pipeline | Focused Verified |
| Establish complete lifecycle provenance | Correct finish production, checkpoint capture/check identity, fresh replay ancestry and full execution-step coverage remain unverified | Unverified |

The consumer relation does not prove the origin of every finish record. A rewrite
removing both completion and all recognizable finish projections also requires
complete execution-step evidence; this unit does not claim that proof. Existing
Core typing and trusted higher-order/imported producer boundaries remain as
recorded above. Full AE/AC acceptance, foreign capture, caught-host-trap cleanup,
pinned library comparison and remaining general allocation obligations stay open.

Validation: all nine new driver tests pass, with required Wasmtime and real
optimized MIR. `cargo fmt --all --check`, `git diff --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass. Mandatory workspace
execution passes 435 backend tests, 95 Core tests, all 80 control integration
tests and the existing cost probes. The nine failing targets and 175 failed test
names exactly match `df46f2d2`, with no added or recovered names. This compares
failure names, not all baseline causes. Logs:
`/private/tmp/psrs-control-finish-structural.log`,
`/private/tmp/psrs-control-finish-workspace.log`,
`/private/tmp/psrs-control-finish-comparison.json`, and
`/private/tmp/psrs-control-finish-clippy.log`. The six ancestry/GC workloads are
remeasured; fingerprints, collection counts and maximum post-GC live bytes match
the preceding helper slice. Explicit upstream type comparison, package
conformance and scoreboards are not rerun for this internal invariant change.
No measured gate number changes.
