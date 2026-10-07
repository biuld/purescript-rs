# Explicit polymorphic arguments in Core

## Contract and repair

The unchanged public `Data.Number.fromString "42.5"` wrapper stopped at P7
Core verification. Its foreign implementation has a `Fn4` signature containing
explicit universal callback and result arguments. P5 retained those arguments,
but Core's shared invariant matcher rejected any flexible nominal parameter
whose replacement was `ForAll`. Constructor field checking had the same blanket
restriction.

Core now retains the whole checked explicit replacement, including its binders.
The shared binding operation still owns occurrence checking and consistency
between repeated parameters. Nominal positions remain invariant; neither
direction admits replacing a universal slot with a specialized function. The
constructor checker uses the same operation for its explicit arguments and
field templates. No inference rule, official source, or library lock changes.

The minimal checked-instantiation regression failed before the repair. Positive
and negative tests cover alpha-equivalent replacements, repeated-parameter
consistency, rigid parameters, specialized functions, constructor fields, and
occurrences hidden beneath quantifiers. A reduced source foreign declaration
with an uncurried newtype and a universal argument now survives Core lowering.
Existing rank-N source acceptance/rejection and mandatory Wasmtime execution
tests both pass, including rejection of inferred impredicative constructors.

## Comparable public replay

Both diagnoses use library revision
`2ee2d1fdaff8a2841824cd1bb5d3fb864f594630` and fingerprint
`fnv1a64-v1:801317111aa80a8f`:

```purescript
module Main where
import Prelude
import Data.Number as Number
import Data.Maybe (Maybe(..))
main :: Int
main = case Number.fromString "42.5" of
  Just value -> if value == 42.5 then 42 else 1
  Nothing -> 1
```

| Checkpoint | First blocker |
| --- | --- |
| Before | P7 Core verification: expression type inconsistent with context |
| After | P8 library linking: `Data.Number.fromStringImpl` has no target implementation |

Raw diagnosis snapshots, replay inputs, and validation logs remain under
`/private/tmp/psrs-polymorphic-*` and `/private/tmp/psrs-number-parsing-*`.
They are not committed. This replay establishes removal of the P7 blocker;
it does not establish parsing behavior or whole-library compilation.

## Rust validation

`cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.
Focused Core tests and the reduced source
regression pass. `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test rank_n`
passes both source and execution tests.

`CARGO_INCREMENTAL=0 PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`
completes all 52 target suites, including doc tests: 1674 passed, three existing
failures, and five ignored. The failures match the preceding checkpoint:

- `dictionary_audit::execution::constrained_dictionary_parameters_precede_ordinary_arguments`:
  the test cannot find its expected MIR constrained function.
- `tests::functions::runs_a_polymorphic_identity_with_a_number`:
  its optimized WAT does not contain the asserted `f64` token.
- `tests::integration::compiles_if_expression_through_cfg_to_structured_wasm`:
  its optimized constant branch does not contain the asserted `br_if` token.

Full workspace validation is not green. No official scoreboard or gate
measurement is changed.

## Remaining work

Implement the missing parsing target with the official pure wrapper preserved.
The library owns prefix recognition and callback behavior; a checked decimal
conversion primitive may own correctly rounded binary64 conversion. Validate
against the pinned official JS FFI, including whitespace, accepted decimal
prefixes, malformed exponents, overflow, underflow, and signed zero. Other
Number foreign slots and whole-standard-library behavior remain separate work.
