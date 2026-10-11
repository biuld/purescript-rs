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
