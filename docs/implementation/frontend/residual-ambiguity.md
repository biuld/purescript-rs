# Residual ambiguity through local bindings

Measured on 2026-10-07.

## Contract and change

The declaration checker owns residual solving, functional-dependency closure,
ambiguity checks, and generalization. Unannotated `let` and `where` bindings are
monomorphic and share its unknowns and wanted constraints. Official
`TypeChecker/Types.hs::inferLetBinding` uses this contract too.

Previously local inference generalized an unused constraint into a local
dictionary parameter. Consequently `f y = let g x = method x in y`, with
`method :: C a => a -> a`, hid the ambiguous `C a` from the enclosing check.
Local inference now retains that obligation for the declaration checker;
the existing ambiguity check reports `AmbiguousTypeVariables` before
generalization. Explicit local `forall` annotations retain polymorphism.
Lexical local environments are restored on both successful and failed inference.

A checked leading `forall` on a whole `let` scopes its definitions as well as
its body. THIR and Core verify this scope. Beta reduction preserves the binder
on the enclosing expression instead of repeating it on the generated local
binding. Direct IR tests accept that scope and reject unrelated free variables.
Erasure fixtures that previously relied on implicit local polymorphism now
declare their `forall` explicitly, preserving the runtime behavior they test.

## Comparable evidence

The baseline is HEAD `11db1757c20b76b5231e033395f000714f279baa` with the preceding
WIT payload changes in the working tree. The package stays pinned to revision
`01d6cd406cdce68a7ea1ec4c26a44793ead34571`, fingerprint
`fnv1a64-v1:b2890fecd9c42aa3`. No package sources or lock entries changed.

`residual-ambiguity-observations.json` retains the input and compiler identities from
the single-file diagnosis snapshots. The ambiguity fixture adds `main = 0` to
the source in `reports_ambiguous_variables_before_generalizing`, so an absent
entry point does not mask the erroneous acceptance.

| Check | Baseline | Result |
| --- | --- | --- |
| Ambiguity fixture diagnosis | Passed compilation | P5 `AmbiguousTypeVariables` |
| `residual_constraints` integration tests | 7 passed, 1 failed | 8 passed |
| Local-constraint driver tests | Existing acceptance fixtures | 14 passed |
| Typechecker unit tests | Existing cases | 135 passed |
| THIR tests, including quantified-local rejection | Existing cases | 28 passed |
| Official differential | Six local-binding sources | All six agree on acceptance and diagnostic code |
| `failing/ConstraintInference.purs` | Annotated `AmbiguousTypeVariables` | Same diagnostic from P5 |
| Polymorphism erasure runtime audit | Implicit-polymorphism fixtures corrected | 21 passed |

The six differential sources cover unused and used constraints, conflicting
monomorphic uses, an explicit polymorphic local, an explicitly constrained local,
and functional-dependency determination. They run through the installed official
`purs`; the corpus assertion uses the official case's annotation separately.
`PolykindGeneralizationLet.purs` remains blocked by an existing P5 kind mismatch
before reaching its annotated type error; this work does not claim coverage of
that case or completion of polymorphic kind support.

Reproduce with:

```sh
cargo test -p psrs-driver --test residual_constraints
cargo test -p psrs-typecheck --lib
cargo test -p psrs-thir --lib
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib let_constraints::
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit::
PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream residual_constraints:: -- --nocapture
```

The L2 rerun remains 72/72 failing agreement and 402/413 passing resolution.
The 11 blocked passing cases remain P3: 6, P0: 4, harness loading: 1.
D-04 and README therefore retain their measurements. The full workspace test
suite was deliberately omitted under the user's focused-validation instruction.
