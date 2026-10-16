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
