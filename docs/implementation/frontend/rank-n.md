# Rank-N Types Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Type inference](../../design/frontend/type-system/type-inference.md)
and [polymorphism and erasure](../../design/backend/fp/polymorphism-and-erasure.md).

**Progress:** In progress. This record separates source acceptance, checked-IR
invariants, and actual runtime evidence. FE-18 completion requires the complete
design contract, including official-suite reconciliation; a rank-2 example
alone is insufficient.

## Acceptance matrix

| ID | Requirement | Required evidence | State |
| --- | --- | --- | --- |
| RN-01 | Quantifiers retain lexical scope beneath arrows and inside type synonyms. | Checked `ForAll` structure, shadowing and substitution tests. | Verified |
| RN-02 | Polymorphic parameters instantiate independently at each use. | Source identity used at Int and Boolean, official comparison, value-sensitive execution. | Verified |
| RN-03 | Expected types drive higher-rank lambda and application checking at arbitrary finite rank. | Rank-3 source and deeper nesting; monomorphic arguments rejected. | Verified |
| RN-04 | Polymorphic record fields remain quantified through construction, projection, and checking. | Accepted identity field, rejected specialized field, execution. | Verified |
| RN-05 | Data and newtype fields remain quantified through construction and patterns. | Source constructor/pattern tests and execution for both forms. | Verified |
| RN-06 | Returned polymorphic values preserve their own callable boundary. | Int-to-polymorphic-identity producer, two distinct result uses, execution. | Verified |
| RN-07 | Captured polymorphic values retain a uniform definition signature. | Closure reads the captured function at Int and Boolean; execution. | Verified |
| RN-08 | Nested constraints elaborate given and wanted dictionaries at the correct expression scope. | Constrained higher-rank argument and lambda, official comparison and execution. | Verified |
| RN-09 | Class methods can accept quantified values and preserve instance checking rigidity. | Higher-rank method source, invalid specialized implementation, execution. | Verified |
| RN-10 | Skolems cannot escape and generalization/substitution respect scopes. | Negative source cases, explicit solver scope regressions, malformed IR rejection. | Verified |
| RN-11 | Checked IR verifiers and optimizations preserve polymorphism and use-site instantiation. | Malformed THIR/Core cases, repeated-variable consistency, optimization regressions. | Verified |
| RN-12 | Source behavior agrees with official PureScript without introducing unrestricted impredicative inference. | Accepted/rejected differential battery plus official rank-N and skolem corpus reconciliation. | Unverified |

RN-04 also covers record patterns. RN-01/RN-03 include shadowed binders,
recursive annotated bindings, rank-4 nesting, and partial instantiation of
quantifiers in both inferred schemes and explicit signatures. RN-12 distinguishes the
accepted `Array (forall a. a -> a)` literal with an expected element type from
the rejected attempt to infer a polymorphic argument for an ordinary generic
`Box` constructor. Array execution uses the target's intrinsic indexer and
checks one projected polymorphic element at both Int and Boolean.

## Evidence and commands

The shared source battery is `crates/psrs-driver/tests/support/rank_n/mod.rs`.
`tests/rank_n.rs` checks source acceptance/rejection and executes accepted
programs; `tests/upstream/rank_n.rs` compares the same cases with official
`purs`. Runtime success must assert the observable result, not just validation
or WAT output.

```sh
cargo test -p psrs-driver --test rank_n source_rank_n
PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream differential_rank_n -- --nocapture
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test rank_n executes_rank_n -- --nocapture
```

Checked on 2026-10-01 with Wasmtime 49.0.1 and official `purs` 0.15.16.
`PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test rank_n` passed,
including both partial quantifier instantiations. The same sources passed
`differential_rank_n` against `/Users/biu/Projects/purescript`. Workspace tests,
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
and `git diff --check` passed in the same tree.

RN-11 additionally covers the THIR and Core verifiers: empty `ForAll`, an
inconsistent scheme instance, closed records with extra fields, rigid open rows,
and one row variable with two residuals are rejected; one repeated residual is
accepted. Global inlining leaves a `ForAll` signature and a body that binds its
own type variable in place. The linker renumbers `TypeVariableId` across modules.
`derive newtype` keeps a shared method quantifier as the scope of its
representation cast. That cast is not `Coercible` evidence.

RN-12 stays unverified. The differential battery agrees with `purs` 0.15.16, and
the official higher-rank and skolem corpus, including its library dependencies,
is not reconciled. FE-18 therefore stays Partial.

## Quantifier consumption review (2026-10-23)

Issue #165 starts from `41a16e25`, with only the pre-existing untracked
`output/` directory. `passing/3125.purs` rejects a qualified inferred lambda
result as `TypesDoNotUnify`; `passing/922.purs` rejects a quantified class
method as the input of a constructor pattern. Both failures also reproduce on
the unmodified pre-Phase-4 HEAD and precede this repair.

Official `Types.hs` instantiates an inferred lambda body before constructing its
arrow. Its `instantiateForBinders` also instantiates a scrutinee when any
alternative requires a monotype. The inference and pattern paths now use the
existing shared `instantiate_all_expr` operation at those boundaries, retaining
constraint evidence and source spans. Checking a declared polymorphic function
result still uses the expected-result path. Variable-only case patterns retain
their existing polymorphic scrutinee behavior.

The four shared cases in `tests/support/rank_n/consumption.rs` cover a qualified
lambda result, constructor and literal scrutinees, and a visible quantified value
retained by a variable pattern and used at both Boolean and Int. They agree with
purs 0.15.16 and execute to the asserted exit 42. The new rejection
`inferred_lambda_does_not_return_rank_n` prevents an inferred lambda from gaining
an impredicative return type merely because its body carries a polymorphic
ascription. The existing explicit higher-rank return and skolem rejection cases
continue to pass.

Comparable before/after diagnosis uses the same corpus filter, one-program
limit, locked library and 30-second compiler timeout for each of `3125` and
`922`. The snapshots `/tmp/psrs-quantifier-{3125,922}-{before,after}.json`
and their `-compare.json` reports confirm identical inputs and recovery from P5
rejection to completed compilation. This observes the frontend/Core boundary;
canonical artifact-content and expression-lineage evidence remain unavailable.

Final validation passes formatting, default Clippy and `cargo test --workspace`
(2331 passed, zero failed, six ignored boards). The mandatory rank-N execution
battery and all 22 purs differential test functions pass. The five frontend
boards execute separately: L1 904/908 (the four DEC-16 differences), L2 76/76 and
403/413 passing resolution, L3 50/50, L4 65/65, L5 101/101, roles 12/12.

The frozen release harness `/tmp/psrs-quantifier-suite-final`, SHA-256
`5cd4a0087cfb8c39d794012ab85d42ae67efdf378c7cb025a773363bf1e68758`,
remeasures every one of the 365 runtime roots serially: **299/340**, 25 FFI
exclusions, all agreeing exits zero. Relative to the Phase 4 measurement,
`3125` and `922` recover and no case loses agreement. The 41 remaining roots are
26 P5 typecheck, 4 P5 kind check, 5 P3 resolve, 1 P8 closure conversion, 2 P0 lex,
1 runtime assertion trap, 1 guest timeout and BigFunction's incomplete compiler
allocation failure. The locked library, Wasmtime 48.0.1, 10-second guest timeout,
524288-byte engine stack and BigFunction compiler-only 8 GiB bound are unchanged.
`/tmp/psrs-quantifier-runtime-board/{measurement,results,summary}.json` retains
the exact settings, per-program evidence and name-set comparison. The workspace,
frontend, focused and lint logs use the `/tmp/psrs-quantifier-` prefix.

These repairs do not promote RN-12 or the broader frontend/backend feature rows
to Verified. The kind-quantification loss in `4105` remains independently
reproduced and unmodified by this repair.

## Inherited local value aliases (2026-10-23)

Issue #169 starts from `6b6f46cf`, preserving untracked `output/`. The unchanged
`passing/2803.purs` rejects in P5 with `NoInstanceFound (Ring _T355)` at the
unused local `f = (-)`; `%` still names the top-level addition function. The
30-second file diagnosis is `/tmp/psrs-operator-alias-before.json`.

Official `inferLetBinding` retains the inferred value type. Variable synthesis,
including imported and foreign signatures, now freshens the existing scheme without consuming its quantifiers, and locals
retain those quantifiers regardless of visibility. A retained pending method
becomes a dictionary lambda using the same given scope and abstraction operation
as expected qualified types. Ordinary values already carry their dictionary
function. Fresh local lambda unknowns and obligations still belong to the
enclosing declaration; no new local HM generalization or defaulting is added.

Eleven shared accepted cases in `tests/support/rank_n/aliases.rs` cover an ordinary
polymorphic alias, an unused method without instances, distinct method
instantiations, a qualified global alias, a partially visible application and
an alias chain, ordinary/method values retained by variable patterns, and
polymorphic record projection/update and Boolean conditions. A separate shared
two-module program verifies ordinary and qualified aliases across imports. The
shared rejection requires an actual missing instance at use. Existing lambda ambiguity and monomorphic identity rejections remain in
the official differential battery. Mandatory runtime evidence includes those
accepted values and unchanged official `2803`, whose stdout must be `Done`.

All 23 purs differential test functions and mandatory rank-N executions pass.
The unchanged official `2803` also compiles and prints `Done` using purs 0.15.16
and Node; `/tmp/psrs-qualified-alias-purs-2803.json` records every input hash and
the separate compile/runtime outcomes. The unchanged official
`RowPolyInstanceContext` also prints `Done` on both runtimes; its unused local
aliases retain their inherited quantified row constraints. That observation does
not claim execution of its unused state computations.

Comparable diagnosis `/tmp/psrs-operator-alias-before.json` and
`/tmp/psrs-operator-alias-final-v3-after.json` uses the same file inputs, locked
library and 30-second timeout. `/tmp/psrs-qualified-alias-final-v3-compare.json`
confirms compatible inputs and recovery from P5 rejection to completed
compilation; canonical artifact-content and expression-lineage comparisons
remain unavailable.

All 2337 ordinary workspace tests pass with zero test failures; six ignored
boards run separately. The combined `cargo test --workspace` invocation then
exited in rustdoc with E0463 (`psrs_linker` metadata unavailable). An isolated
serial `cargo test --workspace --doc` rerun passes for every workspace crate.
Formatting and default Clippy pass. The final five frontend boards pass:
L1 904/908 (four DEC-16 differences), L2 76/76 and 403/413 passing resolution,
L3 50/50, L4 65/65, L5 101/101 and roles 12/12.

The frozen release harness `/tmp/psrs-qualified-alias-suite-final-v3`, SHA-256
`6a7f268f727e67bd96fbcc6fb8704fd677f81534fa1e3fdbc55f7819ee0c6017`,
executes all 365 runtime roots serially: **302/340**, 25 FFI exclusions, all
agreeing exits zero. Exactly `2803` and `RowPolyInstanceContext` recover from
300/340; no program loses agreement. The remaining 38 roots are 24 P5 typecheck,
3 P5 kind check, 5 P3 resolve, 1 P8 closure conversion, 2 P0 lex, the existing
assertion trap and guest timeout, and BigFunction's incomplete compiler
allocation verdict. The library lock, Wasmtime 48.0.1, 10-second guest timeout,
524288-byte engine stack and compiler-only 8 GiB BigFunction bound are unchanged.
`/tmp/psrs-qualified-alias-runtime-board-final-v3/{measurement,results,summary}.json`
retains the exact settings, outcomes and name-set comparison. Earlier interrupted
runs are separate observations. Final workspace/frontend/fmt logs use the
`final-v3` suffix; the focused and Clippy logs use `boundaries-focused` and
`boundaries-clippy`. RN-12 and FE-18 remain incomplete; this repair does not
establish corpus-wide rank-N coverage.

## Written annotation scopes (2026-10-24)

Issue [#175](https://github.com/biuld/purescript-rs/issues/175) starts from a
clean isolated `b2351e5b`. A written annotation was first probed, then its body
was checked again at the outer use type. This lost its own binder scope when
nested annotations renamed variables. Independently, consuming a compound
polymorphic expression changed its root type without retaining the quantifiers
needed by its internal types and dictionaries.

Leading expected quantifiers and givens now enter their ordinary scope before
an inner annotation is checked. The written value is checked once and retained
in an administrative local binding; the ordinary variable checker adapts a use
of that binding. The same preservation operation serves full instantiation,
head-constraint consumption and visible type application. Definitions keep their
original nested types and evidence, while references and wrapper result types
use distinct scopes from the shared scheme/kind freshener. This introduces no
new impredicative inference or relaxed escape rule.

The former discard-on-success annotation probe is removed. Failed speculation
now restores both solver state and lexical scope through the common owner,
retaining diagnostics only when the caller requests them. The speculation
regressions exercise kind/level/wanted allocations, dictionary local IDs,
annotation variables, givens, warnings and diagnostic retention. Additional
scope tests require lexical locals to be restored and newly introduced skolems
to remain rigid after leaving a given. A functional-dependency counterexample
that both the baseline and first prototype incorrectly accepted is now rejected,
matching purs. The source counterexample is part of the shared rejection battery.

| ID | Requirement | Evidence | State |
| --- | --- | --- | --- |
| AS-01 | Written forall binders scope the checked body and shadow outer binders correctly | Single, nested, record and shadowing cases; scope-exit and rigid escape rejections agree with purs | Verified for covered cases |
| AS-02 | Compound values retain definition scope at every quantifier-consumption entry | Full/partial/skipped visible applications and ordinary monomorphic uses execute; existing returned-value regression remains valid | Verified for covered cases |
| AS-03 | Failed adaptations restore the complete solver and lexical environment | Four shared scope tests preserve allocation counters, kinds, levels, annotations, givens and diagnostics; lexical locals restore and body skolems remain rigid | Verified for covered cases |
| AS-04 | Dictionary-bearing and imported annotations share normal checking and use operations | Qualified/specialized cases and a two-module producer/consumer execute with distinct Boolean and Int dictionaries | Verified for covered cases |
| AS-05 | Adaptation preserves evaluation count and source order | Effect.Ref history 123 and result 42 under mandatory Wasmtime and independent purs/Node | Verified for covered case |
| AS-06 | Original official roots compile, validate and execute without source edits | All three original paths compile and execute through the public CLI; stdout/stderr/exits match purs/Node; full runtime board is separate | Verified for three roots |
| AS-07 | Full native and official gates are reconciled without losing prior agreement | Five frontend boards and the corrected integrated workspace pass; full runtime measurement pending | Unverified |

Nine shared accepted programs in `tests/support/rank_n/annotations.rs` cover
written and nested scopes, records, constraints, specialization, full/partial/
skipped visible application and shadowing. Three shared rejections keep binders
from leaking, reject a rigid specialization, and keep nested skolems rigid after
a given scope exits. The same native and purs
battery also checks the two-module annotation program. Mandatory rank-N
execution passes all four test functions, including these additions and the
existing corpus alias execution. The source/observable comparisons use installed
purs 0.15.16; full Clippy, formatting and the five frontend boards pass for the corrected
implementation after incorporating the aggregate ABI tests.

`tests/annotation_scope.rs` separately compiles ordinary Prelude, Effect.Ref and
Effect.Unsafe wrappers. Its nested annotated value must evaluate once between
writes 1 and 3, then work at Boolean and Int. Native execution returns 42.
`/tmp/psrs-annotation-order-oracle/measurement.json` records the exact same user
source and all 280 official compile inputs, their hashes, compile exit zero,
and Node's `42\n`, empty stderr and exit zero. This compares values under the
project's Int command protocol; it does not assert identical output protocols.

The exact 30-second diagnoses
`/tmp/psrs-annotation-isolated-{before,after}-{single,nested,record,2941}.json`
retain complete source sets and requested traces. All four comparisons verify
identical inputs and library fingerprints. Original `2941` recovers from P5
rejection to complete compilation. The three main-less reductions instead
reach P10's missing-entry diagnostic: their type checks recover, but they are
not executable acceptance cases. Corrected-version repeats under
`/tmp/psrs-annotation-scope-v2-{after,compare}-` verify the same four input
sets and lock, with the same recovered stages and no timeout. Canonical
artifact-content and expression lineage comparisons remain unavailable.

The preserved clean-baseline executable also reproduces the original P5
monotype-finalization blockers in `CheckTypeClass` and `ScopedTypeVariables`;
both compile after the repair. All three unchanged original paths also compile
through the public CLI and execute under Wasmtime with a 524288-byte stack and
10-second guest bound. Their stdout is exactly `Done\n`, stderr empty and exit
zero, matching separate purs/Node executions. Retained components, official JS,
source hashes and the comparison are in
`/tmp/psrs-annotation-scope-v2-original-execution/comparison.json`; unchanged
official JS remains in the original execution directory. This observes the
original main programs; the shared value-sensitive cases separately exercise
the annotated functions and dictionaries. Their paired reports use the
`/tmp/psrs-annotation-adjacent-` prefix. Later baseline invocations observe a
dirty checkout, so their recorded executable identity, matching the earlier
clean baseline, is authoritative for which compiler ran. Controlled reductions
under `/tmp/psrs-annotation-adjacent-reductions` remove the relevant annotation:
both controls pass both compilers; both annotated forms fail before and pass
after; purs accepts all four. These are diagnosis experiments, not modifications
to official acceptance inputs.

The complete-support value/order acceptance also executes successfully with
all 309 locked support sources. Five frontend boards pass: L1 904/908 (four
DEC-16 differences), L2 76/76 with 403/413 passing resolution, L3 50/50,
L4 65/65, L5 101/101, and roles 12/12. Logs use the
`/tmp/psrs-annotation-` prefix. The initial workspace attempt caught an invalid
flat module filename; it is corrected by grouping `expected/mod.rs` and
`expected/value.rs`. No verifier or source-layout rule is weakened.

The first frozen release harness `/tmp/psrs-annotation-suite-final`, SHA-256
`79a71bf4a03aa73fb2a2f86a8c7472029fa967595357d9e80129eedffefee9ba`,
completed all 365 roots and passed its complete inventory audit: **315/340**,
exactly the three named recoveries and no lost agreement. This is evidence for
the first prototype, before the final lexical/given-scope corrections. It does
not establish acceptance of the corrected implementation. Its archived sources,
layout normalization and result decomposition remain under
`/tmp/psrs-annotation-runtime-board-final`; BigFunction retains an incomplete
compiler allocation verdict under its 8 GiB compiler-only bound.

The corrected implementation also incorporates the merged aggregate ABI tests
from master. Its replacement workspace completes 77 test targets with 2369 passed, zero
failed and seven ignored under mandatory Wasmtime. Formatting, full Clippy,
the focused purs comparison and replacement frontend boards pass. The complete
runtime run remains required before publishing a new gate count. The runtime limits, library lock and
checking environments remain unchanged. Published acceptance stays 312/340;
RN-12 and FE-18 remain incomplete.
