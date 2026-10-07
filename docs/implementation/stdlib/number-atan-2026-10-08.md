# Number inverse-tangent acceptance

## Contract and implementation

Starting compiler revision: 957b133 on stdlib/vendor-core-libraries, with a
clean worktree. The starting package was
30f22bdb3b207d908ed00ac779d7b0a0a32cb00d
(fnv1a64-v1:db4b18f8950a407f). A fresh Number.atan 0.0 diagnosis stopped at
P8 library linking because Data.Number.atan had no target implementation.

Wasm has no inverse-tangent instruction. The compiler owns
numberAtan :: Number -> Number. Its HIR identity is appended as 72, preserving
existing intrinsic IDs. Core and CC require Number operand and result types.
MIR calls the scalar export `number_atan` in the shared numeric runtime.
Incorrect foreign binding schemes are rejected before ABI erasure.

The export copies the fdlibm polynomial pinned by libm 0.2.15 and returns the
same bits, including subnormals. Every finite input matches the official
JavaScript Math.atan results used by purescript-numbers, and negative zero
stays negative zero. Positive infinity is pi/2 and negative infinity is
negative pi/2. NaN produces NaN. The operation does not trap. NaN payloads
are not part of the public contract. No constant folding is introduced. A
whole inverse-tangent algorithm does not become a compiler intrinsic.

The published libm routine forces an f32 evaluation on subnormal inputs so a
host can raise the underflow flag. That evaluation writes below the stack
pointer without reserving a frame, which makes the linker's static stack
bound unknown. Wasm has no floating-point status flags. The runtime copy
returns the input unchanged when its magnitude is below 2^-27 and does not
touch the stack pointer.

The numeric runtime retains its formatter, decimal conversion, inverse-cosine,
and inverse-sine exports and adds number_atan. Its reproducible artifact
SHA-256 is
41064c763782dc2a184cfc1a47e0816e8dc0b9f8ea465320cc7c294b60807afe.
The private table still has two funcref slots. The active initializer now
points at function 25. Static stack analysis accepts the artifact and still
measures 1680 bytes inside the existing 65536-byte reserve.

The independent library changes only the foreign slot's explicit binding:
`foreign import "psrs:intrinsic#numberAtan" atan :: Number -> Number`.
Its signature, exports, and all official pure declarations remain unchanged.
The complete-module source verifier checks this transformation against pinned
purescript-numbers v9.0.1 (27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

## Package and runtime evidence

Locked package: f2cdf0341ccb166ed827723ee305095780e3b2b6
(fnv1a64-v1:369a7661425142dc).

```sh
node ../psrs-stdlib/conformance/number-atan.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-atan-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-atan-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-atan-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-number-atan-oracle/Main.purs -o /tmp/psrs-number-atan-locked.wasm
wasmtime run /tmp/psrs-number-atan-locked.wasm
```

The actual pinned official JS atan produces 149 input observations and 298
checks, exercising direct public calls and higher-order calls. Inputs include
both zero signs, unit slopes, both infinities, subnormals, nonfinite values,
and 128 deterministically generated binary64 patterns. Reciprocal observations
distinguish zero signs. NaN checks do not require a payload. Both the
development-package run and the locked-package artifact
(sha256 00008e3259148e8c40d65d0198e06c8f333143f69102fc034f30e888bc462b0c)
return 42 with empty stdout and stderr. Wasmtime is 49.0.2.

The same public atan 0.0 probe now passes against the locked package, in
7701 ms. A fresh Number.atan2 0.0 1.0 probe stops at P8 library linking
because Data.Number.atan2 has no target implementation (7360 ms). Both new
diagnoses use fnv1a64-v1:369a7661425142dc. The earlier atan failure used
fnv1a64-v1:db4b18f8950a407f, so the before and after diagnoses are not a
same-fingerprint compare.

Node tooling: 11 passed, none skipped. The numeric runtime rebuild is
byte-for-byte reproducible.

## Rust validation

Two driver regressions pass with mandatory Wasmtime: public behavior,
including zero signs, both infinities, a higher-order call, and the linked
`number_atan` export, and rejection of invalid foreign contracts. The runtime
unit tests check the public boundary bit patterns and agreement with
libm::atan, including subnormals. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass.

`PSRS_REQUIRE_WASMTIME=1 CARGO_INCREMENTAL=0 cargo test --workspace --offline --no-fail-fast`
finishes all 52 targets: 1703 passed, 3 failed, 5 ignored. The only failing
target is `psrs-driver --lib`. Its three failures are the established
baseline: `constrained_dictionary_parameters_precede_ordinary_arguments`,
`runs_a_polymorphic_identity_with_a_number`, and
`compiles_if_expression_through_cfg_to_structured_wasm`. No new failure
appeared. Full workspace validation is not green because of those baseline
failures. This change does not establish complete Number FFI support or
whole-standard-library runtime behavior.
