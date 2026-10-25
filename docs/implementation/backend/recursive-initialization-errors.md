# Recursive initialization errors

**Recorded:** 2026-10-25 (Asia/Shanghai)\
**Status:** Verified for the established message-only target Error contract.\
**Owner:** #101 / RV-08 in [recursive local values](recursive-local-values.md)\
**Contracts:** [CC IR](../../design/backend/fp/cc-ir.md),
[Effect.Exception](../../design/backend/fp/effect-exception.md)

## Production implementation and evidence

The driver attaches each actual source unit and resolved module name to Core's
immutable `SourceCatalogue` before linking. Source-qualified ranges survive
inlining and capture. The catalogue rejects missing/ambiguous identities,
reversed/out-of-bounds ranges and non-UTF-8 boundaries before mapping a line.
Low-level Core without required location metadata produces an invalid-IR error;
it does not receive a fabricated file or line.

Every recursive getter force supplies a checked String message for that read.
The local access entry preserves binding-name/module context through ordinary
and recursive captures. Initializing has an explicit `RaiseSourceError` arm;
the impossible-state default retains `Unreachable`. Failure leaves Initializing
set, so later reads raise at their own locations without retrying work or
replaying a cached exception. Ready values retain their original identity.

CC's generic raise requires a defined String operand and has no normal result.
Common nested-operation reachability requests the same source-error tag used by
checked throw/catch providers, including modules with no live provider. P9
shares native Throw lowering, payload checks and capability rejection. Budget
exhaustion keeps its separate tag; source catches do not become catch-all.

Four actual purs 0.15.16/Node and frozen-CLI/Wasmtime comparisons agree on exit 0,
empty stderr and byte-exact stdout: direct multiline demand, an inlined Helper
whose filename differs from its module, and escaped getters after either
reentrant demand or an explicit initializer throw. The latter cases distinguish
read lines 15 and 16, retain count 1 across failed reads and count 2 after an
independent initialization. Results and source/compiler hashes are retained in
`/tmp/psrs-stream/initialization-evidence/`; both `reference-summary.json` and
`target-summary.json` accept all four cases. The initial library-tool invocation
could not launch Wasmtime inside the sandbox; its report is retained separately,
and the actual comparison passed outside that launch restriction.

Five maintained driver tests also execute Control unwinding, recovery inside a
runner and a fresh runner after failure. Four backend tests cover provider-free
nested raising/execution, unavailable/non-String operands, disabled EH and MIR
payload rechecking. Catalogue tests exercise identity/bounds/UTF-8 rejection;
the low-level memo test rejects absent source units. The original seven
recursive-value executions remain, with the demand case now checking the
uncaught-source-error contract (exit 1 with empty output), not an engine trap.

Final local validation passes with `PSRS_REQUIRE_WASMTIME=1 cargo test
--workspace`: 78 targets, 2,409 passed, zero failed and seven ignored.
`cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D
warnings` pass. Logs are `raise-workspace-final.log` and
`raise-clippy-final.log` under `/tmp/psrs-stream/`. The library-owned Aff
comparison accepts all twelve programs in `aff-regression-initialization/aff.json`.
The package lock and its source fingerprint are unchanged.

The final 365-root runtime run exits 0 and remains **316/340**, with all agreeing
programs exiting zero, 25 unchanged FFI exclusions and 74 support inputs. No
agreement names or residual failure observations change. It takes 2211.729
seconds with the unchanged four-worker and resource limits. The remaining 24
are 11 P5 typecheck, three P5 kind check, five P3 resolve, two P0 lex, the
`TCOMutRec` assertion trap, the `2288` guest timeout and BigFunction's incomplete
allocation failure (-6, 36.377 seconds under 8 GiB). This closes RV-08, not #101
or the full runtime gate.

`/tmp/psrs-stream/runtime-board-initialization/acceptance-audit.json` verifies
all 439 corpus sources, 1205 compiler input files, 479 unchanged library payload
files, executable hashes, per-root exits and the complete result name set.
`terminal-status.json` retains the observed runner exit 0. The frozen suite
SHA-256 is `cb8d2fb49b2329a8e405a5926ea78921c34e64cb38a2e8441787bbd30558f4bc`;
the frozen public CLI SHA-256 is
`03c5b2bba57d10f62235b89548a5ce1663b59d6c802aec5b0a9e1d6327c54599`.
The pre-validation short run was explicitly interrupted and marked incomplete;
it is not gate evidence. No additional broad `PURESCRIPT_REPO` differential run
is claimed; the four reference comparisons actually executed purs and Node.

The following sections retain the measured pre-fix investigation. Their frozen
artifacts are baseline evidence, not the current production behavior.

## Distinguish the failures

The earlier scalar regression returned from its initializer but skipped the
ready-state write during tail conversion. Commit `656611c0` repairs that
continuation defect; the maintained scalar execution returns 42.

The initialization-demand trap had a different path. A valid source initializer passes a
thunk referring to its own value into a function that immediately calls it:

```purescript
applyThunk :: (Unit -> Int) -> Int
applyThunk thunk = thunk unit

bad :: Unit -> Int
bad _ =
  let value = applyThunk (\_ -> value)
  in value
```

The outer getter writes Initializing (1), enters the initializer, then the
thunk calls the same getter before the initializer returns. No ready-state
write should have occurred yet. The compiler must report initialization demand,
not fabricate a result, retry the initializer, or treat the state as corrupt.

The baseline `cc/lower/letrec/values.rs` emits explicit arms for Uninitialized (0)
and Ready (2), and an `Unreachable` default. Initializing (1) consequently uses
an engine trap. `Effect.Exception.try` selects the native source-error tag and
correctly does not catch an engine trap. That catch policy is not the defect.

## Actual state observation

Artifacts are retained in `/tmp/psrs-stream/trap-analysis/`. The frozen compiler
is `psrs-v2-final`, SHA-256
`321521255dbb46ea5888e6448e9c57d95baf38cb2a53265795e2a6e247b3af94`.
The original WAT execution exits 134 with an unreachable trap. Its innermost
frame is function 439, the same getter also present in the next outer frame.

Two diagnostic-only WAT variants discriminate the state reaching its default
arm. A matching tag deliberately traps with `unreachable`; a nonmatching tag
performs an intentionally out-of-bounds access on an empty private array:

| Diagnostic condition | Actual outcome |
| --- | --- |
| Tag equals 1 | Unreachable trap, exit 134 |
| Tag equals 0 | Array-bounds trap, exit 134 |

`state-probe.json` records the original/variant hashes and complete streams.
This establishes tag 1, rather than merely inferring it from an undifferentiated
trap. The variants change no source or compiler code and are not acceptance
artifacts or a proposed implementation.

## Existing exception transport works

The source imports a live `try`, so its emitted main module already has:

- source-error tag 0 with one non-null GC String payload;
- independent stack-exhaustion tag 1 with no payload.

A diagnostic WAT prototype replaces only the getter's trap with a String payload
and `throw 0`. The unchanged ordinary source `try`/`message` path then prints
`probe: recursive value demanded during initialization`, exits 0, and has empty
stderr. A negative control using `throw 1` exits 1 with no output: source `try`
does not recover that distinct tag. Full results are in
`source-error-prototype.json` and `other-tag-control.json`.

This proves transport and dynamic protection for this program. It does not
implement compiler lowering, plan a tag for programs without a live catch, or
establish source-location/message fidelity. The prototype's diagnostic message
is deliberately not presented as the official error. No production file or
independent stdlib source was changed for these experiments.

## Read location is required

The official purs/Node comparison with a multiline initializer separates its
declaration (line 11) from its recursive read (line 13). The error message names
line 13. `ReadSite.purs`, `readsite-official-build.json`, and
`readsite-node.json` retain the actual source, compiler invocation and output.

The baseline getter calls have no explicit parameters. `read_local_binding` sees
the read expression's range, but the generated getter only retains the binding
range. Raising a fixed message inside it would lose the failing-read location.
A later read of an escaped getter after an initializer failure also needs its
own read location; it must not reuse an initializer error or retry the work.

General provenance must survive Core linking and P7 inlining before rendering
that location. The independently reproduced `origin-probe` already attributes
Helper's bytes 153..165 to a 58-byte Main file. Inferring a location from the
current enclosing function therefore gives a provably wrong result. This is a
shared representation problem, not a recursion-only lookup-table problem.

### Provenance prerequisite at commit `3c14a07a`

[DEC-27](../../decision/DEC-27-source-qualified-ranges.md) defines source-qualified
ranges. Core linking now qualifies expression, declaration, binder and pattern
ranges in its existing conversion. Subsequent copying preserves that origin;
the shared Core verifier and backend error/warning constructors consume it.

The retained `origin-probe` uses unchanged Main and Helper inputs. Before the
change the public diagnosis reports Main bytes 153..165 (outside its 58 bytes).
The rebuilt public CLI now reports Helper bytes 153..165, exactly `unsafeCoerce`,
with the same P8 rejection and diagnostic message. This corrects attribution;
it does not make the incompatible representation supported. The after-report
is `/tmp/psrs-stream/origin-probe/after.json`.

Maintained regressions check both an inlined callee error and an inlined
redundant-alternative warning against the original source text. Range tests
check that moving an already qualified range cannot change its file. At that checkpoint the source
catalogue and failing-read context remained open; no runtime gate was re-measured
or changed by that prerequisite alone.

Provenance validation on 2026-10-25: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and the final serial
`cargo test --workspace` pass (78 targets, 2,398 passed, zero failed, seven
ignored). The retained workspace log is
`/tmp/psrs-stream/origin-workspace-serial.log`. An earlier run's executable
tests passed but rustdoc could not load a dependency after an overlapping CLI
build; all doc tests and then the full workspace passed when rerun serially.
`PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test
recursive_values_execution` also passes all seven executions. That inventory
then contained the initialization-demand trap expectation; passing it preserved
the baseline behavior and did not close RV-08. The production cases above
replace that expectation with catchable raising.

## Implementation boundaries and verification requirements

1. Preserve authoritative source identity/range through linking, substitution,
   inlining, capture/lifting and lowering. Keep synthetic IR origins explicit;
   never invent a real module or line when that information is absent.
2. Extend the common getter-force contract to retain the failing read's context.
   Keep eager initialization, shared identity and no-retry behavior unchanged.
3. Add a generic checked source-error raise at the CC/native boundary, rather
   than manufacturing an unchecked library import. Require a String payload
   and a bottom result; preserve spans and verify malformed operands.
4. Share source-error tag planning with existing checked throw/catch providers.
   A reachable compiler-generated raise must work even without a live catch or
   explicit source throw. Keep the exception capability check and the distinct
   stack-exhaustion channel.
5. Give Initializing an explicit source-error arm. Impossible state tags retain
   their unreachable guard; assertion and invalid-resumption traps do not become
   catchable as a side effect of this repair.
6. Execute the accepted dynamic catch/message comparison, distinct read-site
   cases, failed-initializer/no-retry and escaped-getter cases, disabled-profile
   and malformed-IR checks, the existing seven recursive-value cases, Aff
   regressions and required workspace validation. Re-measure the runtime board
   before recording a new gate number.

The established Error target remains message-only, default-name Error and
stackless, as specified by DEC-24. The transport prototype does not extend that
representation or claim equality with JS ReferenceError names or stack traces.
