# Number and Int rounding acceptance

## Scope and ownership

Starting compiler revision: 820de13 on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was 0e3273498523ab81742ad0751efce5fde55e9794
(fnv1a64-v1:9ff76cd44757e7e6). A fresh diagnosis of Int.floor 42.9 failed at
P8 library linking: Data.Number.floor had no target implementation.

The compiler now owns the generic checked numberFloor/numberCeil primitives,
not library algorithms or Int range policy. Their stable identities are
appended to the HIR registry, preserving existing IDs. Core, CC and MIR check
Number/F64 operands and results; Wasm encodes f64.floor/f64.ceil. Existing
malformed unary-operation tests cover both additions. Driver source tests
execute nonfinite values, fractions, signed zero and Number range above i32,
and reject incorrect foreign binding schemes. Backend tests execute both MIR
optimization paths. No new constant folding is claimed.

The independent library owns the actual ECMAScript round implementation in
PSRS.Number: preserve zero signs in [-0.5, 0], otherwise compare the fractional
part with 0.5 before adding one to the floor. This avoids both Wasm nearest's
ties-to-even and the precision loss from flooring value + 0.5. The
[ECMAScript contract](https://tc39.es/ecma262/2023/multipage/numbers-and-dates.html#sec-math.round)
is checked against actual pinned official JS results. Full-module source
transformations preserve all official pure declarations and signatures.
Data.Int's floor/ceil/round/unsafeClamp wrappers are unchanged.

## Package and behavior evidence

Locked package: 2ee2d1fdaff8a2841824cd1bb5d3fb864f594630
(fnv1a64-v1:801317111aa80a8f). The package owns source adaptations, provenance,
the Node oracle and source-verifier test; the compiler retains its lock and
Rust regression tests.

```sh
node ../psrs-stdlib/conformance/number-rounding.mjs \
  /private/tmp/ps-pkgs/purescript-numbers \
  /private/tmp/ps-pkgs/purescript-integers /tmp/psrs-rounding-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-rounding-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-rounding-runtime
# Also build directly with PSRS_STDLIB_ROOT unset, then execute the artifact.
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-rounding-oracle/Main.purs -o /tmp/psrs-rounding-locked.wasm
wasmtime /tmp/psrs-rounding-locked.wasm
```

- 69 inputs, 207 rounded observations and 414 public Number/Int checks pass:
  both signs of zero, subnormals, nonfinite values, half-integer ties and their
  adjacent values, Int boundaries/overflow, values around 2^52 and maximal
  finite Number values. Development-package and locked-package executions
  return 42 with empty stdout/stderr.
- The preceding 147 public isFinite/trunc checks still pass with exit 42 and
  empty stdout/stderr. Node tooling: 11 passed, zero skipped.
- Complete source audit: 229 modules across 41 packages, 170 identical,
  36 modified and 23 target additions, no absent upstream modules and no
  direct recursive foreign placeholders. All 229 stored vendored hashes match
  their actual files, including one pre-existing stale ST.Partial hash corrected
  without changing its source.
- Fresh locked diagnosis of the original Int.floor case passes. Its library
  fingerprint differs from the baseline, so this is a replay across a package
  update, not a same-fingerprint diagnose --compare result.

Raw JSON reports and Wasm artifacts remain local under /private/tmp. They are
not committed. See the library's docs/number-rounding.md for source ownership
and its conformance command contract.

## Rust validation

cargo fmt --all --check and cargo clippy --workspace --all-targets -- -D warnings
pass. The two new driver source/foreign-contract tests pass under mandatory
Wasmtime. The two backend optimized/unoptimized rounding tests also pass,
including negative-zero floor operands.

PSRS_REQUIRE_WASMTIME=1 cargo test --workspace initially encountered storage
exhaustion while writing test components. Old regenerable incremental caches
were removed, then the entire workspace was rerun with --no-fail-fast. Completed
unit and integration tests record 1670 passed, 3 established failures and
5 ignored. Two doc targets encountered missing dependency artifacts; after
cargo clean -p psrs-runtime, cargo test --workspace --doc passes serially.
All 52 target suites are accounted for after that recovery.

The three established driver failures match the preceding checkpoint:
dictionary_audit::execution::constrained_dictionary_parameters_precede_ordinary_arguments
(MIR constrained function), tests::functions::runs_a_polymorphic_identity_with_a_number
(the optimized WAT no longer contains an unused f64), and
tests::integration::compiles_if_expression_through_cfg_to_structured_wasm
(the optimized constant branch no longer contains br_if). Full workspace
validation is therefore not green. No official scoreboard measurement was run.

The subsequent metadata-only ST.Partial hash refresh preserves all Purs source;
the final package is rechecked through the trusted-loader test and both locked
public API programs. Raw environment-failed logs remain local for diagnosis.

## Remaining work

This is rounding acceptance, not whole-standard-library acceptance. A fresh
public Number.fromString "42.5" reproducer currently stops at P7 Core
verification: Core expression type is inconsistent with its context. The
library-origin span covers runFn4 fromStringImpl in the unchanged official
wrapper. Root cause has not yet been established; do not replace that wrapper
or classify this as a missing parsing FFI implementation until Core checking
is resolved. Other Number FFI remains unsupported. No scoreboard or gate
measurement is changed by this focused behavior evidence.
