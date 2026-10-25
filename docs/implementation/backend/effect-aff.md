# Effect.Aff implementation checklist

Tracking: [#161](https://github.com/biuld/purescript-rs/issues/161).
Governing decision: [DEC-26](../../decision/DEC-26-cooperative-aff-scheduling.md),
[WASI platform](../../design/backend/wasm/wasi-platform-library.md), and the
independent library's source-fidelity contract. Status: implementation and local acceptance verified; publication pending.

Baseline: compiler `5775006bcff7ce5b70df993d96566054e952e363`, library source
commit `aef5c233bb67ef271d2b32433f2fdcc681903603`, fingerprint
`fnv1a64-v1:2889a7ea7d8b0dc2`. Existing `output/` is preserved.
The launch/lift/delay reproducer fails at P8 library linking with
`UnsupportedSource`: `Effect.Aff._pure` has no implementation. The diagnosis,
trace and frozen executable are retained under `/tmp/psrs-aff/baseline/` and
`/tmp/psrs-aff/measurement-before.json`. A constructor-free representational
higher-kinded carrier preserves a real record value in a Wasmtime probe
(exit 42); that proves a representation boundary, not Aff execution.

- [x] AF-01: Preserve official exports, signatures, roles, instances and pure
  bodies; document the nullary Supervisor representation change and each FFI slot.
- [x] AF-02: Generic ready/deferred/pollable event loop with exact deadlines,
  cancellation, one-time resource release and an ordinary command boundary.
- [x] AF-03: Stack-safe sequential machine, strict/eager pure map behavior,
  errors/recovery, source exceptions and repeatable inert computations.
- [x] AF-04: Suspended/running/completed memoized fibers, join/unsubscribe,
  synchronous/duplicate/stale callbacks and reentrant state updates.
- [x] AF-05: Cancellation registration race, pending cancelers, nested masks,
  acquire/use/finalizer outcomes, recovery exclusion and waiting for cleanup.
- [x] AF-06: Parallel map/application/race, left/right ordering, first failure,
  pending-loser cleanup and cancellation of cancellation observers.
- [x] AF-07: Supervision, child removal, suspended children, parent termination,
  nested scopes and cleanup failure behavior.
- [x] AF-08: Library-owned pinned JS/public Wasmtime conformance, real waits,
  bounded live allocation/stack stress, and neighboring Effect/Console regressions.
- [x] AF-09: Required compiler/tool validation, source audit, lock, full affected
  scoreboard and D-04 updates. No foreign or required slice remains unimplemented.

## Requirement-to-test evidence

| Requirement | Evidence | Observed contract |
| --- | --- | --- |
| AF-01 | `audit-v6-final/inventory.json`, official Aff source diff | All 17 slots are nonrecursive typed replacements; only the nullary Supervisor declaration changes outside those slots. Public exports/signatures/roles/instances/pure bodies remain. |
| AF-02 | `aff-target/EventLoop`, `aff/LongDelay`, Console regression | One release on fire/cancel, queued revocation, double cancellation, 5,000-task batches, timer fairness/normalization and a real six-second interval. |
| AF-03 | `aff/Main`, `aff/Lifecycle` | Recovery and Effect errors, 20,000 recursive steps, strict reusable Pure map, bracket outcomes and pinned finalizer outcome restoration. |
| AF-04 | `aff/Main`, `aff/Lifecycle`, `aff/Reentrant`, `aff/RegistrationThrows` | Suspended/mapped/applied memoized fibers, join revocation, late observation, duplicate/stale callbacks and construction-time exceptions. |
| AF-05 | `aff/Lifecycle`, `aff/ReentrantHandlers`, `aff-target/Registration` | Cancellation waits for masked acquire/release; shared continuation publication retains reentrant interrupts; actual cancelers install and construct once. |
| AF-06 | `aff/Main`, `aff/Lifecycle`, `aff/Reentrant`, `aff/Stress` | Parallel application and race wait for loser cleanup, preserve pinned two-failure observations, and cancel a 1,000-layer tree; a 2,000-layer successful tree is stack-safe. |
| AF-07 | `aff/Supervision`, `aff/Lifecycle`, supplemental killed-supervisor comparison | Registration-order cleanup of 5,000 children, nested scopes, suspended children, parent normal/failure/cancellation and failing-child-finalizer behavior. |
| AF-08 | `conformance-v9-final/aff.json`, `bounded-heap-v4-final/run.json`, `heap-negative-control-v2.json`, `console-v4-final/report.json`, `npm-v3-final.log` | Required real execution, positive and negative outcomes, a fixed GC reservation with a non-collecting negative control, adjacent Console behavior and 19 passing tool tests. |
| AF-09 | Final workspace log and full runtime-board audit | Ordinary and required-runtime workspace runs each pass 77 targets / 2385 tests; fmt and Clippy pass. The complete audited board is 315/340 with no gained or lost agreement. |

Evidence paths are relative to `/tmp/psrs-aff/`. The maintained program names
above are in `psrs-stdlib/conformance/`; the supplemental public-API source is
retained in that repository's ignored `docs/evidence/aff-killed-supervisor-v2/`.
Its JS comparison and target runner records are in `killed-supervisor-v2/`.

## Runtime model and observations

Final library commit `b817f3daf34247360216b48f10c14d8d14ccca3e` has content identity
`fnv1a64-v1:acd7fdd2f2a78577` and is pinned by the default compiler lock. The compiler has no Aff-specific intrinsic,
IR node, runtime dispatch, or syntax change. The source inventory measures
50 packages / 323 modules: 236 identical, 43 modified, 44 target additions,
zero absent official modules and zero detected direct self-recursions.

Types owns the explicit nodes, phases, bindings, unwind frames, tokens and
supervisor membership. Storage owns state publication; Machine/Unwind own
sequential execution and masking; Fiber owns results/observers; Parallel and
Supervisor own their ordinary algorithms. Driver wires delayed callbacks
without module cycles. Aff/ParAff retain their official foreign-data declarations and use the existing
applied-opaque erased reference; only nullary Supervisor changes to an abstract
GC carrier, without adding a public constructor. The separate constructor-free carrier probe returns 42,
establishing that representation boundary independently of Aff execution.

User callbacks run through Defer/Computing outside state transformations;
publication rereads current state. The generic native Host.defer also delays
Effect construction under the active source handler. Consequently registration
construction throws cannot revive a completed fiber through an old callback,
and a first callback result survives a subsequent throw. Target tests cover
reentrant cancellation before canceler installation and during its construction.

Continuation processing and recursive Effect walks are stack-safe. Recursive
construction must itself be delayed until interpretation; strict applicative
operands would otherwise traverse an entire tree or queue before executing its
first request. Deferred promotion and revocation sequence recursive work through
Effect binds. Parallel leaf sets share tree structure rather than copying growing
lists. Completed fibers clear observers, continuations and scope references;
already-observed cancelled results do not enqueue unnecessary failure guards.

The final maintained comparison executes twelve target programs: nine public
success programs with exact official-JS output, two target-only scheduler
contracts, and one unhandled-error program using the existing stackless target
diagnostic contract. The 6,001 ms delay takes 6.095 s on Node and 6.236 s on
Wasmtime including startup. The public stress runs both deep parallel trees,
128 simultaneous timers and 10,000 successive fibers under a 512 KiB stack.
Supervision separately cleans 5,000 actual children in registration order.

The same Stress program passes through the common runner with a non-moving
32 MiB GC reservation, no growth reservation, the copying collector and the
same stack bound. The identical final Wasm with the null collector traps with
`allocation size too large`; the recorded checksum remains unchanged. Actual
reclamation is required for the positive result.
This bounds the observed GC reservation, not whole-process/JIT RSS. The
[pinned engine contract](https://github.com/bytecodealliance/wasmtime/blob/v48.0.1/crates/wasmtime/src/config.rs#L3336)
states which collector reclaims cyclic garbage. Tool identities, including purs
0.15.16 and Wasmtime 48.0.1, are retained in `tool-identities.json`.

Pinned JS returns the left error in the measured two-failure Alt case even when
the right fails first, and restores the body outcome after finalizer failure;
the target preserves those observations. Reentrant cancellation while async
registration installs is a separate target contract: unchanged JS throws
`TypeError: step is not a function`, whereas typed Pending waits for the actual
canceler and invokes it once. The reference execution is retained in
`registration-reference.json`; that case is not counted as JS agreement.

## Compiler and corpus validation

The frozen release harness matches the preceding board byte for byte. The
complete four-worker run finishes in 2107.098 seconds: 315/340 agree, all agreeing
exits zero, with 25 unchanged FFI exclusions and no gained/lost agreement.
All 365 roots, 74 support sources, 439 corpus sources, 1197 archived compiler
build inputs and 479 package payload files are accounted for. The remaining
25 retain their decomposition: 11 P5 typecheck, three P5 kind check, five P3
resolve, one P8 closure conversion, two P0 lex, TCOMutRec's assertion trap,
2288's guest timeout, and BigFunction without a compiler verdict. BigFunction
aborts allocation at the existing compiler-only 8 GiB limit after 37.011 seconds;
that is incomplete compilation, not a successful or rejected compiler result.
`runtime-board-final/acceptance-audit.json` verifies the complete retained run.
The interrupted 181-root `runtime-board-pre-fidelity/` is not gate evidence.

Both issue-required workspace modes pass separately, each with 77 test targets,
2385 passing tests, zero failures and seven ignored scoreboards. The required
Wasmtime mode actually executes the runtime tests. The ignored L6 board is run
separately above; other ignored scoreboards are outside this unchanged frontend
scope. Formatting and all-target Clippy with warnings denied pass. No Rust
source changes are part of this item. Logs:

- `workspace-tests-ordinary-final.log` and `workspace-validation-ordinary.json`
- `workspace-tests-v2-final.log` and `workspace-validation-v2.json`
- `fmt-v2-final.log` and `clippy-v2-final.log`

The optional broad `PURESCRIPT_REPO` differential command was not run; this
item adds no compiler syntax or diagnostic. Its public API oracle instead
compiles pinned official libraries with purs 0.15.16 and executes their actual
JS, with the target comparisons recorded above.

## Final source identity

The compiler lock selects final library commit
`b817f3daf34247360216b48f10c14d8d14ccca3e` /
`fnv1a64-v1:acd7fdd2f2a78577`. Runtime/workspace measurements use frozen commit
`88b62f1dd99f20050aca02086b9fd95dc7f28352`; the final commit adds only an update
to `docs/native-effect.md`. The lib and conformance Git trees and both manifest
objects are identical, and all 479 compiler-consumed package files match their
archived SHA-256 values. The only consumer input change is the lock's revision
metadata. `doc-only-revision-equivalence.json` and
`final-lock-equivalence-audit.json` retain these exact checks.

After selecting the final lock, a fresh default-path build without a development
override executes the original launch/lift/delay input with exact
`before\nafter\n` output and exit zero. The earlier failed diagnosis remains
separate baseline evidence. The source audit permits only the nullary Supervisor
representation change outside original value FFI slots; Aff/ParAff retain the
shared applied-opaque reference convention. Internal producers return actual
Value/concrete supervisor types, and opaque conversions belong to the FFI slots.

All AF requirements have implementation and execution evidence. The broader
BE-21/L6 gate stays Partial; #161 Corpus cases remain zero because no passing
root imports Aff. The local changes have not been pushed or merged, and the
GitHub issue remains In Progress pending publication. Existing `output/` is
preserved. The next roadmap item must be selected from the live board after
this item is handled.
