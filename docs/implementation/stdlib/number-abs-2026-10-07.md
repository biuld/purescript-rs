# Number absolute-value acceptance

## Contract and implementation

Starting compiler revision: 131a1fe on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was 4c4795a9c87496242398554b83f665062e3af2fd
(fnv1a64-v1:5e52e376007f1a23). A fresh Number.abs (-42.5) diagnosis stopped at
P8 library linking because Data.Number.abs had no target implementation.

The compiler now owns numberAbs :: Number -> Number. Its HIR identity is
appended, preserving existing intrinsic IDs. Core and CC require Number
operand/result types, MIR requires F64, and Wasm emits f64.abs. Incorrect
foreign binding schemes are rejected before ABI erasure. Existing malformed
CC/MIR unary-operation checks cover the new operation. No additional runtime
artifact, host binding, library-name recognition, integer conversion, or
constant folding is introduced.

The independent library changes only the foreign slot's explicit binding:
`foreign import "psrs:intrinsic#numberAbs" abs :: Number -> Number`.
Its signature, exports, and all official pure declarations remain unchanged.
The complete-module source verifier checks this transformation against pinned
purescript-numbers v9.0.1 (27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

Wasm absolute value clears the sign bit while preserving finite magnitudes
and NaN payloads. Negative zero becomes positive zero, either infinity becomes
positive infinity, and NaN remains NaN. The public oracle checks NaN value
behavior rather than a JS payload identity. See the
[Wasm scalar contract](../../design/backend/fp/scalars-and-primitives.md).

## Package and runtime evidence

Locked package: e0679521275277c0b005b2509d9e0a8570715c5e
(fnv1a64-v1:4e357ed0c7f9f10f).

```sh
node ../psrs-stdlib/conformance/number-abs.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-abs-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-abs-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-abs-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-number-abs-oracle/Main.purs -o /tmp/psrs-number-abs-locked.wasm
wasmtime /tmp/psrs-number-abs-locked.wasm
```

The actual pinned official JS abs produces 153 input observations and 306
checks, exercising direct public calls and higher-order calls. Inputs include
both zero signs, subnormals, minimum normal values, fractions, magnitudes
beyond i32, maximal finite values, infinities, NaN, and 128 deterministically
generated binary64 patterns. Reciprocal observations distinguish zero signs.
Both development-package and locked-package executions return 42 with empty
stdout/stderr. The final package also passes the existing 304 parsing checks
and 414 public Number/Int rounding checks, returning 42 with empty output.

The complete source inventory records 230 modules across 41 packages:
170 identical, 36 modified and 24 target additions, no absent upstream modules
and no direct recursive foreign placeholders. This inventory does not approve
all existing adaptations or establish behavior of every API. Node tooling:
11 passed, none skipped. All 230 vendored source hashes match their inventory.
Raw reports, generated inputs, runtime artifacts and
validation logs remain local under /private/tmp and are not committed.

## Rust validation

The two new driver regressions pass with mandatory Wasmtime: public behavior
and rejection of invalid foreign contracts. Backend execution passes with
optimization enabled and disabled, checking negative-zero canonicalization
and a negative magnitude beyond i32.

`CARGO_INCREMENTAL=0 PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`
completes all 52 target suites, including doc tests: 1686 passed, three existing
failures, and five ignored. The failures match the preceding checkpoint:

- `dictionary_audit::execution::constrained_dictionary_parameters_precede_ordinary_arguments`:
  the test cannot find its expected MIR constrained function.
- `tests::functions::runs_a_polymorphic_identity_with_a_number`:
  its optimized WAT does not contain the asserted f64 token.
- `tests::integration::compiles_if_expression_through_cfg_to_structured_wasm`:
  its optimized constant branch does not contain the asserted br_if token.

Full workspace validation is not green because of these baseline failures.
There is no new failing test in this run.

`cargo fmt --all --check`, `git diff --check`, and
`CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings`
pass.

## Remaining work

The original public abs reproducer now compiles. The package fingerprint
changed, so this is replay across a package update rather than a comparable
same-fingerprint diagnose --compare measurement.

A fresh Number.sqrt 4.0 reproducer stops at P8 library linking because
Data.Number.sqrt has no target implementation. This is absolute-value
acceptance, not complete Number FFI or whole-standard-library acceptance.
No official scoreboard or gate measurement changes.
