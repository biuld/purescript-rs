# Number inverse-sine acceptance

## Contract and implementation

Starting compiler revision: 450759c on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was
e05d4f765c949d50832b7d01b5cf992b5efc903e
(fnv1a64-v1:eccd4891551858bb). A fresh Number.asin 0.0 diagnosis stopped at
P8 library linking because Data.Number.asin had no target implementation.

Wasm has no inverse-sine instruction. The compiler owns
numberAsin :: Number -> Number. Its HIR identity is appended as 71, preserving
existing intrinsic IDs. Core and CC require Number operand and result types.
MIR calls the scalar export `number_asin` in the shared numeric runtime.
Incorrect foreign binding schemes are rejected before ABI erasure.

The export is pinned libm 0.2.15, the fdlibm polynomial. Finite inputs in the
closed interval [-1, 1] match the official JavaScript Math.asin results used
by purescript-numbers, and negative zero stays negative zero. Finite inputs
outside that interval, infinities, and NaN produce NaN. The operation does not
trap. NaN payloads are not part of the public contract. No constant folding is
introduced. A whole inverse-sine algorithm does not become a compiler
intrinsic.

The numeric runtime retains its formatter, decimal conversion, and inverse
cosine exports and adds number_asin. Its reproducible artifact SHA-256 is
7e697803a15ed67e812051d78036e50a1213401296e48895d6cb7dceb27f36f7.
The private table still has two funcref slots. The active initializer now
points at function 24. Static stack analysis accepts the artifact and still
measures 1680 bytes inside the existing 65536-byte reserve.

The independent library changes only the foreign slot's explicit binding:
`foreign import "psrs:intrinsic#numberAsin" asin :: Number -> Number`.
Its signature, exports, and all official pure declarations remain unchanged.
The complete-module source verifier checks this transformation against pinned
purescript-numbers v9.0.1 (27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

## Package and runtime evidence

Locked package: 30f22bdb3b207d908ed00ac779d7b0a0a32cb00d
(fnv1a64-v1:db4b18f8950a407f).

```sh
node ../psrs-stdlib/conformance/number-asin.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-asin-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-asin-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-asin-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-number-asin-oracle/Main.purs -o /tmp/psrs-number-asin-locked.wasm
wasmtime run /tmp/psrs-number-asin-locked.wasm
```

The actual pinned official JS asin produces 149 input observations and 298
checks, exercising direct public calls and higher-order calls. Inputs include
both zero signs, the closed interval endpoints, values one ulp outside that
interval, subnormals, nonfinite values, and 128 deterministically generated
binary64 patterns. Reciprocal observations distinguish zero signs. NaN checks
do not require a payload. Both the development-package run and the
locked-package artifact
(sha256 c41a980e0e75412e84e217c37643145d71873e0164fd27a3cfbdb9a0305cb011)
return 42 with empty stdout and stderr. Wasmtime is 49.0.2.

The same public asin 0.0 probe now passes against the locked package, in
7825 ms. A fresh Number.atan 0.0 probe stops at P8 library linking because
Data.Number.atan has no target implementation. The package fingerprint
changed, so the before and after diagnoses are not a same-fingerprint compare.

Node tooling: 11 passed, none skipped. The numeric runtime rebuild is
byte-for-byte reproducible.

## Rust validation

Two driver regressions pass with mandatory Wasmtime: public behavior,
including zero signs, a higher-order call, and the linked `number_asin`
export, and rejection of invalid foreign contracts. The runtime unit test
checks the fdlibm boundary bit patterns. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.

`PSRS_REQUIRE_WASMTIME=1 CARGO_INCREMENTAL=0 cargo test --workspace --offline --no-fail-fast`
finishes all 52 targets: 1699 passed, 3 failed, 5 ignored. The only failing
target is `psrs-driver --lib`. Its three failures are the established
baseline: `constrained_dictionary_parameters_precede_ordinary_arguments`,
`runs_a_polymorphic_identity_with_a_number`, and
`compiles_if_expression_through_cfg_to_structured_wasm`. No new failure
appeared. Full workspace validation is not green because of those baseline
failures. This change does not establish complete Number FFI support or
whole-standard-library runtime behavior.
