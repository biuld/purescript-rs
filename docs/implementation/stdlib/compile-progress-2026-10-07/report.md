# Stdlib compilation progress

This slice repairs three compiler contracts and implements the public Bounded
foreign slots in the independent library. It starts at compiler `7f93266` and
stdlib `8aaaffa`. The final library pin is
`18987e3fec9025830ce576e4815a6d7cf58bacb4`, content fingerprint
`fnv1a64-v1:b7f12516354db01c`.

## Compiler contracts

- P3 qualified scopes combine disjoint imports. Ambiguity depends on canonical
  declaration identity at lookup or pseudo-module re-export, rather than a
  blanket ban on shared aliases. Six official-purs differential cases cover
  disjoint members, repeated canonical declarations, unused conflicting scopes,
  and value/type/re-export ambiguity.
- P4 guard lowering creates fallthrough helpers only for rows that can be
  entered after the first guarded row. Previously unused copies introduced
  fresh inferred result constraints in Data.Enum, Data.Enum.Generic and
  Data.List.Lazy. The repair preserves scoped dictionary checking; a missing
  dictionary is still rejected. Three official-purs cases and value-sensitive
  guarded/fallthrough executions cover the boundary.
- P8 coverage analysis checks the default matrix for incomplete finite
  signatures before constructing a finite missing witness. Witness construction
  tracks the type path, so binary recursive products cannot continually grow
  the query columns. Missing uninhabited constructors still require analysis of
  observed fields. Core tests cover either constructor order, redundancy,
  missing witnesses, and uninhabited recursive products. Source execution
  covers both binary-tree constructor orders. Data.Map.singleton/size now
  compiles and returns 42 under Wasmtime; the prior reproducer aborted with a
  stack overflow in usefulness recursion.

## Library boundary

Data.Bounded changes only its six typed foreign slots and one target import.
PSRS.Bounded owns i32 bounds and binary64 infinities over existing checked
scalar primitives. The official pure code, APIs, classes and instances remain.
DEC-16 justifies Char's U+10FFFF scalar upper bound instead of JS's U+FFFF.
The pinned-source generator verifies the exact allowed transformation and reads
actual upstream JS constants. The 18 checks include three Enum integration
observations, which are not claimed as an upstream Enum differential oracle.
`bounded-run.json` records Wasmtime exit 42 and empty stdout/stderr.

## Validation and limits

The L2 annotations scoreboard reports M2 72/72 and passing resolution 403/413.
The ten remaining blockers are six P3 and four P0; there are no missing-library
or harness-loading blockers. Nineteen sibling modules load successfully.
The package audit covers 224 modules in 41 packages: 172 identical, 34 modified,
18 target additions, no upstream module omitted, and no direct-self-recursion
replacement. Audit categories do not by themselves approve target differences.
Library Node tooling tests: 8 passed.

`diagnosis-checkpoints.json` distinguishes the initial P3 failure (165
diagnostics), alias-only P5 failure (7 diagnostics), and the guard-fixed full
import acceptance (137559 ms with a 240-second cohort). The same 211-import
source with unused integer main timed out under 120 seconds after the coverage
repair. Different timeout cohorts and library fingerprints are not presented as
compatible diagnosis comparisons. The final locked-package remeasurement passes in 172290 ms under the
240-second cohort, with no diagnostics. It retains the same input fingerprint.

Compile acceptance of an unused main does not prove every exported API executes
or every declaration is retained in the executable. The focused List.length,
Set.singleton/size, Map.singleton/size and Enum range executions establish only
those paths. The final locked-package String and Int parsing probes still fail explicitly
at P8 library linking: `Data.String.CodeUnits.length` and
`Data.Int.fromStringAsImpl` have no target implementation (9489 ms and
13615 ms). Some Number operations, Lazy and Effect.Ref also retain foreign slots
without target support in the earlier survey. This slice does not close the entire stdlib.

The real Map runtime fixture is:

```purescript
module Main where
import Prelude
import Data.Map as M
main :: Int
main = if M.size (M.singleton 1 2) == 1 then 42 else 1
```

Formatting and strict workspace clippy pass. The backend library suite passes
403 tests; the driver library suite passes 685 tests with the three failures
below (mandatory Wasmtime, 233.18 seconds). The trusted-order loader and all
new guard/recursive-product runtime regressions pass. The original HEAD independently
reproduces three driver-library failures: `constrained_dictionary_parameters_precede_ordinary_arguments`,
`runs_a_polymorphic_identity_with_a_number` and
`compiles_if_expression_through_cfg_to_structured_wasm`. Their optimized-artifact
assertions are a pre-existing validation baseline, not a passing full workspace.
The complete `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`
run finishes with 1658 passed, 3 failed and 5 ignored tests, exit 101.
The run also reports E0463 dependency-artifact load failures in backend and
driver rustdoc targets. A sequential
`cargo test -p psrs-backend -p psrs-driver --doc` recheck passes, exit 0,
without source changes. These artifact errors do not remain unresolved. All
other targets and doc-tests pass.
The 19 upstream differential groups pass. Ignored scoreboards remain
unmeasured here except for the separately executed L2 board. `validation.json`
records the result and the independently verified baseline failures.
