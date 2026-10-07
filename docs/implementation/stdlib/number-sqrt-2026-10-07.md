# Number square-root acceptance

## Contract and implementation

Starting compiler revision: 0aab025 on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was
7cace2a6b4808b36b01559e88d3ebc351a3b7426
(fnv1a64-v1:1fd8c04ec63c6ff0). A fresh Number.sqrt 4.0 diagnosis stopped at
P8 library linking because Data.Number.sqrt had no target implementation.

The compiler now owns numberSqrt :: Number -> Number. Its HIR identity is
appended as 69, preserving existing intrinsic IDs. Core and CC require Number
operand and result types, MIR requires F64, and Wasm emits f64.sqrt. Incorrect
foreign binding schemes are rejected before ABI erasure. The operation does not
trap: a negative finite value or negative infinity becomes NaN, NaN stays NaN,
positive infinity stays positive infinity, and negative zero stays negative
zero. No additional runtime artifact, host binding, library-name recognition,
or constant folding is introduced.

The independent library changes only the foreign slot's explicit binding:
`foreign import "psrs:intrinsic#numberSqrt" sqrt :: Number -> Number`.
Its signature, exports, and all official pure declarations remain unchanged.
The complete-module source verifier checks this transformation against pinned
purescript-numbers v9.0.1 (27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

## Package and runtime evidence

Locked package: ef004e75998995ea8ac00840e36c1d9b71226b25
(fnv1a64-v1:19dabd2d6d4033b4).

```sh
node ../psrs-stdlib/conformance/number-sqrt.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-sqrt-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-sqrt-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-sqrt-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-number-sqrt-oracle/Main.purs -o /tmp/psrs-number-sqrt-locked.wasm
wasmtime run /tmp/psrs-number-sqrt-locked.wasm
```

The actual pinned official JS sqrt produces 155 input observations and 310
checks, exercising direct public calls and higher-order calls. Inputs include
both zero signs, subnormals, minimum normal values, exact squares, magnitudes
beyond i32, maximal finite values, infinities, NaN, and 128 deterministically
generated binary64 patterns. Reciprocal observations distinguish zero signs.
Both the development-package run and the locked-package artifact
(sha256 1098509065918c064f27cbe5a6d75508a08b7de5d304a565c94796d680f73a0e)
return 42 with empty stdout and stderr. Wasmtime is 49.0.2.

The same public sqrt 4.0 probe now passes against the locked package. A fresh
Number.acos 1.0 probe stops at P8 library linking because Data.Number.acos has
no target implementation. The package fingerprint changed, so the before and
after diagnoses are not a same-fingerprint compare.

The complete source inventory records 230 modules across 41 packages:
170 identical, 36 modified and 24 target additions, no absent upstream modules
and no direct recursive foreign placeholders. This inventory does not approve
all existing adaptations. Node tooling: 11 passed, none skipped.

## Rust validation

Backend execution of f64.sqrt passes with optimization enabled and disabled,
checking that the square root of negative zero stays negative and that the
square root of 2^32 is 65536. Two driver regressions pass with mandatory
Wasmtime: public behavior, including a higher-order call, and rejection of
invalid foreign contracts. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.

`PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast` finishes all
52 targets: 1693 passed, 3 failed, 5 ignored. The only failing target is
`psrs-driver --lib`. Its three failures are the established baseline:
`constrained_dictionary_parameters_precede_ordinary_arguments`,
`runs_a_polymorphic_identity_with_a_number`, and
`compiles_if_expression_through_cfg_to_structured_wasm`. No new failure
appeared. Full workspace validation is not green because of those baseline
failures.

## Remaining work

This is square-root acceptance, not complete Number FFI or whole-standard-library
acceptance. No official scoreboard or gate measurement changes.
