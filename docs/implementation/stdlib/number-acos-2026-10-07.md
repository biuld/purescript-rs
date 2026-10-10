# Number inverse-cosine acceptance

## Contract and implementation

Starting compiler revision: b300392 on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was
ef004e75998995ea8ac00840e36c1d9b71226b25
(fnv1a64-v1:19dabd2d6d4033b4). A fresh Number.acos 1.0 diagnosis stopped at
P8 library linking because Data.Number.acos had no target implementation.

Wasm has no inverse-cosine instruction, so this is not an `f64.sqrt`-style
opcode. The compiler owns numberAcos :: Number -> Number. Its HIR identity is
appended as 70, preserving existing intrinsic IDs. Core and CC require Number
operand and result types. MIR calls the scalar export `number_acos` in the
shared numeric runtime. Incorrect foreign binding schemes are rejected before
ABI erasure.

The export is pinned libm 0.2.15, the fdlibm polynomial. Finite inputs in the
closed interval [-1, 1] match the official JavaScript Math.acos results used
by purescript-numbers. Finite inputs outside that interval, infinities, and
NaN produce NaN. The operation does not trap. NaN payloads are not part of the
public contract. No constant folding is introduced. A whole inverse-cosine
algorithm does not become a compiler intrinsic.

The numeric runtime retains number_to_string and number_from_decimal and adds
number_acos. Its reproducible artifact SHA-256 is
092c1aca5a1270c0c792568da6c230ebdd25fcf536e64f17729315db75e3c683.
The private table still has two funcref slots. The active initializer now
points at function 22. Static stack analysis accepts the artifact and still
measures 1680 bytes inside the existing 65536-byte reserve.

The independent library changes only the foreign slot's explicit binding:
`foreign import "psrs:intrinsic#numberAcos" acos :: Number -> Number`.
Its signature, exports, and all official pure declarations remain unchanged.
The complete-module source verifier checks this transformation against pinned
purescript-numbers v9.0.1 (27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

## Package and runtime evidence

Locked package: e05d4f765c949d50832b7d01b5cf992b5efc903e
(fnv1a64-v1:eccd4891551858bb).

```sh
node ../psrs-stdlib/conformance/number-acos.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-acos-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-acos-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-acos-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-number-acos-oracle/Main.purs -o /tmp/psrs-number-acos-locked.wasm
wasmtime run /tmp/psrs-number-acos-locked.wasm
```

The actual pinned official JS acos produces 149 input observations and 298
checks, exercising direct public calls and higher-order calls. Inputs include
both zero signs, the closed interval endpoints, values one ulp outside that
interval, subnormals, nonfinite values, and 128 deterministically generated
binary64 patterns. A zero result is distinguished by its reciprocal. NaN
checks do not require a payload. Both the development-package run and the
locked-package artifact
(sha256 2b2ba1dd721be0b84db2b7a8eeb5385c974ea701fbd2cdb4a4fa656b5e02abb2)
return 42 with empty stdout and stderr. Wasmtime is 49.0.2.

The same public acos 1.0 probe now passes against the locked package, in
8522 ms. A fresh Number.asin 0.0 probe stops at P8 library linking because
Data.Number.asin has no target implementation. The package fingerprint
changed, so the before and after diagnoses are not a same-fingerprint compare.

Node tooling: 11 passed, none skipped. The numeric runtime rebuild is
byte-for-byte reproducible.

## Rust validation

Two driver regressions pass with mandatory Wasmtime: public behavior,
including a higher-order call and the linked `number_acos` export, and
rejection of invalid foreign contracts. The runtime unit test checks the
fdlibm boundary bit patterns. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.

`PSRS_REQUIRE_WASMTIME=1 CARGO_INCREMENTAL=0 cargo test --workspace --offline --no-fail-fast`
finishes all 52 targets: 1696 passed, 3 failed, 5 ignored. The only failing
target is `psrs-driver --lib`. Its three failures are the established
baseline: `constrained_dictionary_parameters_precede_ordinary_arguments`,
`runs_a_polymorphic_identity_with_a_number`, and
`compiles_if_expression_through_cfg_to_structured_wasm`. No new failure
appeared. Full workspace validation is not green because of those baseline
failures. This change does not establish complete Number FFI support or
whole-standard-library runtime behavior.
