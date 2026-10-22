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
