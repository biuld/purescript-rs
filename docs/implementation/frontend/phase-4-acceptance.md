# Phase 4 Acceptance Review

**References:** [D-04](../../design/D-04-suite-roadmap.md),
[type inference](../../design/frontend/type-system/type-inference.md),
[primitives](../../design/frontend/type-system/prim.md).

**Status:** Verified for the mapped Phase 4 gate. Mapped diagnostic agreement
does not prove the complete frontend feature matrix or runtime corpus is supported.

## Starting point

The review started at `509c30676746b758e582a18cc0e1b641bc4c86df` on `master`.
The existing untracked `output/` directory was preserved. The locked library is
`618bede18e4bf7fc57206c057781a12fcef2b3b4`, fingerprint
`fnv1a64-v1:44b22a61d1b2b19e`. The tools are purs 0.15.16 and Wasmtime 48.0.1.

## Requirements and evidence

| Issue | Requirement | Evidence and remaining work |
| --- | --- | --- |
| #99 | M4 diagnostics, solved typed holes, and concrete remaining feature gaps | The annotations board agrees on L4 65/65, including `HoleInferredType` 10/10. Broader feature gaps remain in FE-08..FE-13. |
| #100 | Official expression-kind, polykind, row-kind and local-scope cases | The annotations board agrees on L3 50/50, including all original 48 cases and the two quantification codes. |
| #98 | Deriving diagnostics, fundep cases and method-local constraints | L5 agrees 101/101. The deriving and rank-N differential batteries execute against purs; the fundep wildcard is accepted. Broader deriving and RN-12 acceptance remain separate obligations. |
| #123 | Binder visibility and every named passing/failing VTA consumer | The VTA regression battery checks invisible binders, chains, class heads, skips, kind equations and official failing codes. `VisibleTypeApplications`, `VTAsClassHeads`, `4535`, and `4500` all type-check with the locked library. `4500` and `4535` also compile, validate and execute, printing `Done` with exit 0. |
| #77 | Full agreement on the mapped L4/L5 gates | The annotations board agrees L4 65/65 and L5 101/101 (166 measured cases). Format, workspace (2331 passed, zero failed, six ignored boards), Clippy, all five frontend boards and the complete runtime remeasurement pass their operational checks. |

## Corrections required by the review

The original `4500.purs` failed at P5 with `Show (forall v. Reflectable "asdf"
v => v)`. The record-field inference path had retained the remaining binders of
a visible application. Official `Types.hs`'s `inferWithinRecord` instantiates
names, constructors and visible applications of them, while preserving explicit
polymorphic ascriptions. Source records and updates now use that rule;
`MatchProduct` retains its independent scrutinee contract. Open-record fields
with no expected field type use the same inference rule. Unannotated arrays
instantiate their elements, while contextual arrays retain checked polymorphic
elements. Seven shared cases agree with purs and execute to the asserted exit 42.

The local VTA case also exposed missing kind-reference scope for a local
signature's written binders. Its body now sees those binders and the next local
binding does not. The kind regression tests both acceptance and non-leakage.

`Data.Reflectable` had no registered compiler rule. Its canonical class now
uses the existing validated compiler-interface registry and ordinary dictionary
terms for Symbol, Int, Boolean and Ordering reflection. It shares the existing
Ordering constructor registry. Unknown types retain ordinary constraint
behavior; unrelated same-named classes and malformed interface contracts are
rejected. The result type is decided by the primitive and checked by the common
verifier. Ten literal observations, including both signed Int boundaries,
agree with purs and execute to exit 42. Reflected integers are checked against
the target Int range.

The resulting `Show ?v` obligation could precede the reflection that determines
`?v`. The common wanted solver now reaches a fixed point of successful searches
before reporting or generalizing remaining obligations. Failed trials roll back
through the existing transaction mechanism. No reflection-specific retry or
improvement table is used.

A row-alignment return-type alias also repairs the default Clippy
`type_complexity` failure without changing row semantics.

## Reproduction and validation

Evidence logs are under `/tmp/psrs-phase4-*`; these paths are session artifacts,
not repository dependencies. The comparable diagnosis used:

```sh
cargo run -- diagnose --corpus passing --filter 4500 --limit 1 \
  --out /tmp/psrs-phase4-4500-before.json --timeout 20
cargo run -- diagnose --corpus passing --filter 4500 --limit 1 \
  --out /tmp/psrs-phase4-4500-final.json --timeout 20
cargo run -- diagnose --compare /tmp/psrs-phase4-4500-before.json \
  /tmp/psrs-phase4-4500-final.json
```

The comparison confirms identical inputs and cohort and reports recovery from
P5 rejection to compilation. It observes the driver's frontend/Core boundary;
it does not establish canonical artifact-content or expression lineage evidence.

Frontend boards were remeasured with `PSRS_ORACLE=annotations` and
`cargo test -p psrs-driver --test suite -- --ignored --nocapture --skip
l6_runtime_scoreboard`: L1 904/908 (the four DEC-16 differences), L2 76/76,
passing resolution 403/413, L3 50/50, L4 65/65, L5 101/101, and roles 12/12.
The focused acceptance command uses `PSRS_REQUIRE_WASMTIME=1` and the
`rank_n`, `record_instantiation`, `reflectable`, `visible_type_application`,
and `upstream` integration targets. Format and default Clippy pass.

The final `cargo test --workspace` exits 0: 2331 passed, zero failed, six
ignored boards, including successful doc-tests. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` exit 0. The five ignored
frontend boards execute separately using the frozen release harness; the sixth
board executes once per program root with mandatory Wasmtime. The focused
acceptance battery passes 50 test functions, including all 22 purs differential
tests, with no skipped runtime observations.

## Runtime remeasurement and attribution

On 2026-10-23 (Asia/Shanghai), the frozen release harness
`/tmp/psrs-phase4-suite-final`, SHA-256
`8a8a04b105b54c440ec085d4a3b65d8e53e818cc506d496cf5fc4e8bad260557`,
executes every one of the 365 roots serially. Results are **297/340**, 25 FFI
exclusions, all agreeing exits 0. The unchanged guest timeout is 10 seconds and
the explicit engine stack limit is 524288 bytes. BigFunction retains the previous
compiler-only 8 GiB bound with an unrestricted runtime wrapper. It actually
fails allocation; the runner self-check succeeds, and no compiler verdict or
acceptance is fabricated.

The 43 remaining roots split into 28 P5 typecheck, 4 P5 kind check, 5 P3 resolve,
1 P8 closure conversion (`Stream`), 2 P0 lex (DEC-16), the `TCOMutRec` assertion
trap, the `2288` guest timeout and BigFunction's incomplete compiler verdict.
The exact commands, binary digest, settings and per-program outputs are in
`/tmp/psrs-phase4-runtime-board-final/{measurement,results,summary}.json` and the
neighboring logs. The orchestration is the earlier serial scoreboard driver,
with a newly frozen harness and new evidence directory; it does not modify
corpus sources or change the scoreboard classification.

Compared with the 2026-10-21 measurement, 13 cases recover:
`3114`, `4500`, `4535`, `IntToString`, `KindUnificationInSolver`,
`PolykindBindingGroup1`, `PolykindGeneralization`,
`PolykindGeneralizedTypeSynonym`, `PolykindInstantiation`, `PolykindRowCons`,
`SolvingReflectable`, `VTAsClassHeads`, and `WildcardInInstance`.
Three lose agreement: `3125`, `4105`, and `922`. The net gain is 10.

An independent compiler built from `git archive` of the unmodified starting HEAD
reproduces all three losses on byte-identical program inputs; `4105` includes its
`Lib` support source explicitly. They precede this change. The same baseline
rejects `3114`, `4500`, `IntToString` and `SolvingReflectable`, establishing four
compile recoveries in this review. The other nine already compile at the
starting HEAD; their runtime recovery relative to the older measurement must
not be credited to this patch. See `/tmp/psrs-phase4-head-regressions.json` and
`/tmp/psrs-phase4-head-recoveries/summary.json`. These focused baseline checks
are compile evidence, not an independently measured full runtime denominator.
The baseline `VTAsClassHeads` compile uses a separate 120-second observation;
its initial 20-second timeout is retained as an incomplete observation.

Phase 4's mapped gate is verified. Passing-program type/kind defects, RN-12,
complete deriving reconciliation and the broader backend/ABI/runtime gates
remain open in their owning feature rows. No broader feature row is promoted
merely because its mapped failing cases agree.
