# Contextual application results and Symbol reification

**Design:** [Type inference](../../design/frontend/type-system/type-inference.md).

## Baseline and owner

The public `Data.Symbol.reifySymbol` callback may use its locally supplied
`IsSymbol` evidence, but cannot return a proxy at an arbitrary fixed symbol.
The unchanged negative source

```purescript
bad :: String -> Proxy "fixed"
bad value = Symbol.reifySymbol value (\proxy -> proxy)
```

was rejected as `EscapedSkolem`, while the pinned official `purs` rejects it as
`TypesDoNotUnify`. Before checking the callback, P5 synthesized an unknown
result for the application without using the written `Proxy "fixed"` context.
Binding that unknown to the callback's rigid symbol triggered the shared escape
guard before the concrete result mismatch could be checked.

Application checking now constrains a callable's result against a monomorphic
expected result before checking the argument. Result instantiation and wanted
dictionary evidence use the existing scheme operations; evidence is applied to
the completed application after the argument check. Synthesis without a result
context is unchanged. Contexts containing universal or qualified binders retain
synthesis followed by subsumption, preserving the rejection of impredicative
constructor instantiation. The shared skolem-escape binding rule is unchanged.

This belongs to ordinary bidirectional application checking, independently of
Symbol names, library paths, or backend representation. The minimal shared
fixture uses only `(forall a. a -> r) -> r` and an `Int` result.

## Focused evidence

- The shared rank-N acceptance/rejection battery and mandatory Wasmtime runs
  pass, including a constant callback result and a returned constrained value
  that needs dictionary evidence. Existing impredicative rejection and genuine
  inferred-parameter skolem escape remain covered.
- Two explicit `purs` differential tests pass: the complete shared battery and
  the minimal rigid-result diagnostic comparison.
- The independent Symbol oracle passes forty value observations, dynamic input,
  one callback invocation, nested/captured dictionaries, record/closure results,
  strict construction and repeated execution of a returned action, and failure
  propagation. Missing evidence and symbol escape agree with official codes.
- L4 remains 39/50 and L5 remains 77/97 on the same official failing selections
  before and after the checker change. L3 remains 39/48.

The before/after diagnosis reports are `/tmp/psrs-symbol-escape-before.json` and
`/tmp/psrs-symbol-escape-after.json`; the library report is
`/tmp/psrs-symbol-reification-verified/symbol.json`. The independent package
commit is `6fcef6ccbab1f3459af2573bbef4ae25b86852d4`, fingerprint
`fnv1a64-v1:5fcbb57525c63eb1`, in `/tmp/psrs-symbol-reification-stdlib`.
The package preserves every original pure `Data.Symbol` definition and replaces
only its private JavaScript identity foreign slot with the existing typed
`Unsafe.Coerce` interface. Its public-source comparison checks the full exact
diff against pinned prelude v6.0.1. The local package commit is not pushed.

The diagnosis comparison confirms the same cohort, input and observed
environment, with the P5 category changing from `EscapedSkolem` to
`TypesDoNotUnify`. It reports no difference in the observed pass prefix;
canonical artifact-content comparison and build-toolchain compatibility remain
unavailable. The comparison is `/tmp/psrs-symbol-escape-comparison.log`.

The pre-existing Symbol reflection differential now uses an unchanged pinned
`Symbol.purs` fixture alongside its official `Symbol.js`, rather than feeding
the target-adapted source to `purs` without its new target import. Both files
come from prelude commit `f4cad0ae8106185c9ab407f43cf9abf05c256af4`, paths
`src/Data/Symbol.purs` and `src/Data/Symbol.js`. The PureScript SHA-256 is
`8302d378edfcd58fbe1aadcf95c22f8637533de188b99ca2a0e43147e8957ec4`.
Target execution and full source fidelity remain independently checked by the
library oracle.

## Integration validation

The final 2026-10-17 run uses the isolated package above and required Wasmtime:

```sh
PSRS_REQUIRE_WASMTIME=1 PURESCRIPT_REPO=/Users/biu/Projects/purescript \
  PSRS_STDLIB_ROOT=/tmp/psrs-symbol-reification-stdlib cargo test --workspace
cargo fmt --all --check
PSRS_STDLIB_ROOT=/tmp/psrs-symbol-reification-stdlib \
  cargo clippy --workspace --all-targets -- -D warnings
```

All pass: 2,037 tests passed, zero failed, five ignored scoreboards executed
separately. The final workspace log is
`/tmp/psrs-symbol-context-workspace-final.log`; all twenty official differential
tests execute and pass. The earlier workspace failure is retained in
`/tmp/psrs-symbol-context-workspace.log`: it expected `EscapedSkolem` for an ST
action checked against a concrete `Int`. Official differential evidence now
establishes `TypesDoNotUnify` for that mismatch, while a separate inferred-result
ST escape remains rejected with `EscapedSkolem`. The nine region tests pass.
The temporary missing-import failure in the old Symbol differential is retained
in `/tmp/psrs-symbol-context-rank-and-upstream-final.log`; its pinned-source fix
is included in the final green workspace run.

The release runtime board measured 273/340 at this change's acceptance, with
exactly the same 67 failure paths and reasons; the current number lives in the
[runtime recheck](../runtime-program-scoreboard.md#symbol-and-contextual-checking-integration-recheck)
and the D-04 gate table. All L1–L5 boards retain their prior counts.

This API acceptance does not close #124 or establish full standard-library
coverage. `Test.Assert.checkThrows` remains blocked on exception/trap recovery
outside the default target capability profile; #95 is marked Blocked.
