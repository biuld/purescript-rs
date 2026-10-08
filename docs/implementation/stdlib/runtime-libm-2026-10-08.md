# Numeric runtime: upstream libm without a red zone

## Contract and implementation

The numeric runtime pins unmodified `libm` 0.2.15 instead of `fpmath` 0.1.1.
Inverse sine, inverse cosine, inverse tangent, sine, cosine, tangent,
exponential, logarithm, and power call the dependency directly. Remainder
calls `libm::fmod`, replacing the private significand-reduction implementation.
The JavaScript `pow` exceptions, signed-zero minimum/maximum, sign, and NaN
classification remain runtime-owned semantic wrappers. The existing `atan2`
wrapper retains its documented exponent-gap cutoff of 60 for compatibility;
this migration does not change that policy to `libm::atan2`.

`force_eval` uses a volatile read of a temporary to force floating-point
exception evaluation. LLVM can place that temporary below the stack pointer
without publishing a frame (a red zone). The current linker intentionally
rejects that unrecognized access rather than reporting an incomplete bound.
Both artifact build scripts now apply `RUSTFLAGS=-C no-redzone=yes` to the
runtime **and its dependencies**. The generated code reserves explicit frames,
so the existing stack analyzer can measure them. No upstream math source or
linker verification rule is changed, and `force_eval` is retained.

The governing scalar contracts are in
[scalars and primitives](../../design/backend/fp/scalars-and-primitives.md).
Node comparison still distinguishes special-value semantics from ordinary
finite one-ulp differences; this change claims no universal Node bit identity.
The earlier local-copy reports are historical measurements, not measurements
of this artifact.

## Baseline and artifact evidence

The starting checkout was `stdlib/vendor-core-libraries` at
`4e53f83a1db33b129ef70c6b094dc7a6ebb64510`, with existing uncommitted work.
Before this change, the embedded fpmath artifact failed
`stack::tests::the_runtime_artifact_has_a_measured_static_bound` with
`unrecognized stack-pointer write makes the bound unknown`.

An isolated upstream libm probe separately reproduced the original red-zone
rejection: the default build failed with `unrecognized stack-pointer access`,
while the build with red-zone use disabled passed. The two probe builds agreed
on 110198 executed Wasm calls across eleven functions, using boundary values
and a deterministic 10000-value binary64 sample (seed `0x5eed5a17`). NaNs were
compared by class; all other results were compared by bits. This checks the
build-option change on that sample, not exhaustive numerical accuracy.

The rebuilt, packaged runtime artifact is 49204 bytes, with SHA-256
`75cdd69833e058d014942eafdcd668eb9150e8d098a791871c327daf7bb20c73`.
Its measured stack bound is 1680 bytes within the 65536-byte stack reserve.
The private table has two slots, with function 54 initialized at slot 1.
Catalog provenance and the verifier fixture record the new artifact identity.
`tools/check-reproducible.sh` reproduced the packaged artifact byte-for-byte.

## Validation

`cargo test -p psrs-runtime -p psrs-linker` passed: 12 runtime tests and
45 linker tests across the library, compose, guest, and plan targets.
The public `Data.Number.atan` regression now executes both signs of the
smallest subnormal through the compiler and Wasmtime, checking that the
positive input is nonzero and both results preserve their exact values.

With `PSRS_STDLIB_ROOT=/Users/biu/Projects/psrs-stdlib` and
`PSRS_REQUIRE_WASMTIME=1`, `cargo test -p psrs-driver --lib number_` passed
36 tests, including the public subnormal regression. The development package
is used explicitly because the existing lock does not yet expose all thirteen
remaining Number foreign bindings. No stdlib source or compiler lock change
is part of this migration.

The library-owned official-FFI oracle commands are:

```sh
node ../psrs-stdlib/conformance/number-atan.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /private/tmp/psrs-libm-atan-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /private/tmp/psrs-libm-atan-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /private/tmp/psrs-libm-atan-runtime
node ../psrs-stdlib/conformance/number-rest.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /private/tmp/psrs-libm-rest-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /private/tmp/psrs-libm-rest-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /private/tmp/psrs-libm-rest-runtime
```

Both oracle runs passed against development-package fingerprint
`fnv1a64-v1:bca0ace40bd0ccac` using Wasmtime 49.0.2, with exit 42 and empty
stdout/stderr. Atan exercised 149 inputs through 298 direct/higher-order checks;
the remaining functions exercised 1340 observations through 2680 checks.
The latter generator retains its eight documented one-ulp exclusions, so the
result is scoped to its asserted checks. The emitted application SHA-256 values
were `6a2264410e33b1518f9486ef484c1ec888a225e40c41decad763dfea1045107a`
for atan and `ec8c88cf6acb8dc4865c061f1aa9b5248b23ff13e12988ba319dc6592a71c7e0` for the remaining functions.

Formatting (`cargo fmt --all --check`), strict workspace Clippy
(`cargo clippy --workspace --all-targets -- -D warnings`), shell syntax checks,
and `git diff --check` passed.

The full workspace command was run with the same development package and
mandatory Wasmtime:

```sh
PSRS_STDLIB_ROOT=/Users/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 \
  cargo test --workspace --no-fail-fast
```

It exited 101. The driver library had 712 passing tests and four failures;
all other workspace test targets passed. One failure was the Show trace test's
old literal artifact digest. That assertion was updated to the rebuilt digest.
The other three match the previously recorded baseline:

- `dictionary_audit::execution::constrained_dictionary_parameters_precede_ordinary_arguments`:
  the expected constrained MIR function is absent.
- `tests::functions::runs_a_polymorphic_identity_with_a_number`:
  optimized WAT does not contain the asserted `f64` text.
- `tests::integration::compiles_if_expression_through_cfg_to_structured_wasm`:
  optimized WAT does not contain the asserted `br_if` text.

The initial migration did not immediately repeat the full workspace suite
after the digest-only test correction. The structure cleanup below records
the subsequent full rerun.

After that correction,
`PSRS_STDLIB_ROOT=/Users/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::show::`
passed all six tests, including the pinned-digest trace assertion and the
repeated-formatting stack test. The remaining three baseline failures were
not changed by this work. The complete workspace log is
`/private/tmp/psrs-libm-workspace-tests.log`; oracle execution reports are in
the two output directories shown above.

## Module structure cleanup

The thin inverse-cosine, inverse-sine, and inverse-tangent exports and their
existing boundary tests are consolidated in `src/number.rs`, alongside the
other scalar math exports and numeric semantic wrappers. The three one-function
modules are removed. Public Rust re-exports and raw Wasm export names are
preserved. The custom `atan2` compatibility algorithm remains in `src/atan2.rs`
and calls `libm::atan` directly; decimal parsing and formatting remain separate.
The rebuild after this cleanup reproduces the same packaged artifact
byte-for-byte, preserving its digest, layout, and stack-bound evidence.

After the cleanup, formatting, strict workspace Clippy, and artifact
reproducibility checks passed. The full workspace command above was repeated
with mandatory Wasmtime and exited 101 solely for the same three baseline
failures: the driver library had 713 passed and 3 failed; all other workspace
targets passed. All 12 runtime tests passed, including the three relocated
inverse-function tests. The complete rerun log is
`/private/tmp/psrs-runtime-structure-tests.log`.
