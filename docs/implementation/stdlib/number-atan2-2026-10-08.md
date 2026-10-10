# Number four-quadrant inverse-tangent acceptance

## Contract and implementation

Starting compiler revision: e8b5b9a on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was
f2cdf0341ccb166ed827723ee305095780e3b2b6
(fnv1a64-v1:369a7661425142dc). The preceding inverse-tangent diagnosis recorded
that Number.atan2 0.0 1.0 stopped at P8 library linking because
Data.Number.atan2 had no target implementation.

Wasm has no two-argument inverse-tangent instruction. The compiler owns
numberAtan2 :: Number -> Number -> Number. Its HIR identity is appended as 73,
preserving existing intrinsic IDs. Core and CC require Number operands and a
Number result. MIR calls the scalar export `number_atan2` in the shared
numeric runtime. Incorrect foreign binding schemes are rejected before ABI
erasure.

The argument order is `y` then `x`, matching official Math.atan2. The export
uses the fdlibm exponent-gap cutoff of 60. A larger gap returns a signed
half-pi, and a negative `x` with a gap below -60 contributes zero before the
pi adjustment. Moderate ratios call the checked inverse-tangent primitive.
libm 0.2.15's own atan2 uses a wider gap. On 5721 pairs, that routine had 31
finite mismatches against Math.atan2, all one ulp. The cutoff of 60 had 0
finite mismatches and 0 NaN-payload mismatches on the same pairs. The pairs
were the edge product, an exponent sweep, and 256 random binary64 patterns.
Either NaN produces NaN. The operation does not trap. NaN payloads are not
part of the public contract. No constant folding is introduced. A whole
inverse-tangent algorithm does not become a compiler intrinsic.

The numeric runtime retains its formatter, decimal conversion, inverse-cosine,
inverse-sine, and inverse-tangent exports and adds number_atan2. Its
reproducible artifact SHA-256 is
e295fd40bcb247ee77b8890107b2dea74838ded7b383895ce8db7c428d0e7ac2.
The private table still has two funcref slots. The active initializer now
points at function 27. Static stack analysis accepts the artifact and still
measures 1680 bytes inside the existing 65536-byte reserve.

The independent library changes only the foreign slot's explicit binding:
`foreign import "psrs:intrinsic#numberAtan2" atan2 :: Number -> Number -> Number`.
Its signature, exports, and all official pure declarations remain unchanged.
The complete-module source verifier checks this transformation against pinned
purescript-numbers v9.0.1 (27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

## Package and runtime evidence

Locked package: 767ffd7f5bbdbf1a2e1bc568b2dce88deeb5ce88
(fnv1a64-v1:fe305ab57e585eca).

```sh
node ../psrs-stdlib/conformance/number-atan2.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-atan2-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-atan2-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-atan2-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-number-atan2-oracle/Main.purs -o /tmp/psrs-number-atan2-locked.wasm
wasmtime run /tmp/psrs-number-atan2-locked.wasm
```

The actual pinned official JS atan2 produces 289 input observations and 578
checks, exercising direct public calls and higher-order calls. Inputs include
quadrant boundaries, both zero signs, both infinities, subnormals, large
ratios, nonfinite values, and 64 deterministically generated binary64 pairs.
Reciprocal observations distinguish zero signs. NaN checks do not require a
payload. Both the development-package run and the locked-package artifact
(sha256 59a9f6c3a30e84e307ef4d9b1ed970805c3026933ba7e87fde7cd9400e46883a)
return 42 with empty stdout and stderr. Wasmtime is 49.0.2.

The same public atan2 0.0 1.0 probe now passes against the locked package, in
7758 ms. A fresh Number.cos 0.0 probe stops at P8 library linking because
Data.Number.cos has no target implementation (7575 ms). Both new diagnoses use
fnv1a64-v1:fe305ab57e585eca. The earlier atan2 failure used
fnv1a64-v1:369a7661425142dc, so the before and after diagnoses are not a
same-fingerprint compare.

Node tooling: 11 passed, none skipped. The numeric runtime rebuild is
byte-for-byte reproducible.

## Rust validation

Two driver regressions pass with mandatory Wasmtime: public behavior,
including quadrant boundaries, zero signs, infinities, a large ratio, a
higher-order call, and the linked `number_atan2` export, and rejection of
invalid foreign contracts. The runtime unit tests check the public boundary
bit patterns, agreement with libm::atan2 on moderate ratios, and the large
ratio results that match Math.atan2. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.

`PSRS_REQUIRE_WASMTIME=1 CARGO_INCREMENTAL=0 cargo test --workspace --offline --no-fail-fast`
finishes all 52 targets: 1707 passed, 3 failed, 5 ignored. The only failing
target is `psrs-driver --lib`. Its three failures are the established
baseline: `constrained_dictionary_parameters_precede_ordinary_arguments`,
`runs_a_polymorphic_identity_with_a_number`, and
`compiles_if_expression_through_cfg_to_structured_wasm`. No new failure
appeared. Full workspace validation is not green because of those baseline
failures. This change does not establish complete Number FFI support or
whole-standard-library runtime behavior.
