# Remaining Number foreign acceptance

The implementation and artifact measurements below describe the earlier local
copies. The current runtime calls unmodified libm 0.2.15 with red-zone use
disabled; see [the runtime migration](runtime-libm-2026-10-08.md).

## Contract and implementation

Starting compiler revision: 4e53f83 on stdlib/vendor-core-libraries. The starting
package was 767ffd7f5bbdbf1a2e1bc568b2dce88deeb5ce88
(fnv1a64-v1:fe305ab57e585eca). The preceding four-quadrant inverse-tangent
diagnosis recorded that Number.cos 0.0 stopped at P8 library linking because
Data.Number.cos had no target implementation.

This batch binds the thirteen remaining Data.Number foreign slots in one
scalar runtime. Wasm has no sine, cosine, tangent, exponential, logarithm, or
power instruction. Minimum and maximum stay in that runtime as well. HIR
identities are appended from 74 through 86, preserving existing intrinsic IDs,
including retired floor IDs 26 and 27. Core and CC require the checked Number
schemes. `isNaN` is Number to Boolean. `nan` and `infinity` are the only
nullary foreign primitives, and an empty parameter list stays empty. MIR calls
the scalar exports below. Incorrect foreign binding schemes are rejected
before ABI erasure.

None of the thirteen operations trap. NaN payloads are not a public contract,
except that `isNaN` is true for every NaN. No constant folding is introduced.
A whole numeric library does not become a compiler intrinsic. Each binding is
one scalar export:

| Binding | Export | Behavior |
| --- | --- | --- |
| numberSin, numberCos, numberTan, numberExp | number_sin, number_cos, number_tan, number_exp | Local copy of the pinned libm 0.2.15 fdlibm routines. `force_eval` is omitted because it stores below the stack pointer. Returned bits follow those routines. Infinities produce NaN for the three trigonometric operations. Exponential overflow produces infinity and underflow produces zero. |
| numberLog | number_log | `libm::log`. A negative argument produces NaN. Either signed zero produces negative infinity. |
| numberPow | number_pow | `libm::pow`, then the JavaScript exceptions: a NaN exponent produces NaN, and ±1 raised to an infinity produces NaN. An exponent of zero still produces 1, including a NaN base. |
| numberMin, numberMax | number_min, number_max | NaN if either argument is NaN. A zero minimum is negative when either zero is negative. A zero maximum is negative only when both zeros are negative. This is not IEEE minNum. |
| numberSign | number_sign | NaN and both zeros are unchanged. Every other finite or infinite value becomes ±1. |
| numberRemainder | number_remainder | JavaScript `%` through `libm::fmod`: the exact remainder, with a zero result taking the dividend's sign. An infinite dividend or a zero divisor produces NaN. |
| numberIsNaN | number_is_nan | Number to Boolean, returned as i32 0 or 1. |
| numberNan, numberInfinity | number_nan, number_infinity | Canonical NaN `0x7ff8000000000000` and positive infinity. Negative infinity is negation. |

On the oracle input set below, the built artifact and the pinned official
Number FFI disagree by one ulp on exactly eight inputs, and on no other input
in that set. A separate 2000-draw sample from seed `0x5eed5a17` (2000 unary
values, then 2000 pairs on the continued stream) has further one-ulp gaps and
no larger gaps: sine 17, cosine 17, tangent 13, and logarithm 1. Exponential,
sign, `isNaN`, minimum, maximum, power, and remainder had zero one-ulp
disagreements in that sample. The eight oracle gaps are not patched. The
sample does not establish bit identity with `Math` for every input.

The numeric runtime retains its earlier exports and adds these thirteen. Its
reproducible artifact SHA-256 is
2323f3ed445e17fb19a4c6832dd0470afe98cd83d74102a0d17dfca36fbfbbe6.
The artifact is 48836 bytes. The private table still has two funcref slots.
The active initializer now points at function 49. Static stack analysis
accepts the artifact and still measures 1680 bytes inside the existing
65536-byte reserve. Argument reduction for magnitudes that need the 64-bit
coefficient table is compiled only for the host; the wasm32 artifact uses the
32-bit table. Host bit checks of that reduction are not the wasm oracle.

The independent library changes only the thirteen foreign slots' explicit
bindings, for example
`foreign import "psrs:intrinsic#numberCos" cos :: Number -> Number`.
Signatures, exports, and all official pure declarations remain unchanged,
including `isFinite`, `fromString`, and `round`. The complete-module source
verifier checks this transformation against pinned purescript-numbers v9.0.1
(27d54effdd2c0e7a86fe356b1cd813dca5981c2d).

## Package and runtime evidence

The lock is still 767ffd7f5bbdbf1a2e1bc568b2dce88deeb5ce88
(fnv1a64-v1:fe305ab57e585eca). The development package used below is that
commit plus the thirteen uncommitted bindings. Its content fingerprint is
fnv1a64-v1:bca0ace40bd0ccac. That fingerprint is not the lock, and it does
not match the earlier cosine diagnosis, so the two diagnoses are not a
same-fingerprint compare.

```sh
node ../psrs-stdlib/conformance/number-rest.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-rest-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-rest-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-rest-runtime
```

The pinned official FFI produces 1340 observations and 2680 checks. Each
asserted check is a direct public call and a higher-order call. Inputs are
the nonfinite values, both zero signs, selected magnitudes, and 24
deterministically generated binary64 values; binary operations use the first
16 of those values as anchors. NaN checks use inequality. Zero results use a
reciprocal so the sign is visible. The eight one-ulp inputs are recorded in
`observations.json` and are not asserted. Compilation took 44.7 seconds.
Wasmtime 49.0.2 executed the artifact
(sha256 c94db1569cdb01741b5aa3c72bd1b358d5e29d974beff0dc1b5becdea9f6851c)
in 1.34 seconds, with exit 42 and empty stdout and stderr.

A public probe that calls sine, cosine, tangent, exponential, logarithm,
`isNaN`, NaN, infinity, minimum, maximum, power, remainder, and sign passes
against the same development package, in 18668 ms. Node source verification
of the thirteen bindings passed. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` passed on the final
sources. One workspace run, with `PSRS_STDLIB_ROOT` set to the development
package and `PSRS_REQUIRE_WASMTIME=1`, finished with exit 101. Its driver
library target had 713 passed and 3 failed, and those three are the existing
baseline: a missing constrained MIR function, optimized WAT without `f64`,
and optimized WAT without `br_if`. The same run rejected a flat
`rem_pio2_large.rs` sibling; that file now lives at `fdlibm/rem_pio2/large.rs`,
and `cargo test -p psrs-cli --test source_layout` then passed. The workspace
suite was not repeated after that move or after the artifact digest update.
This acceptance does not establish whole-stdlib runtime completion. The lock
still names the previous package, so a locked-package load does not see
these bindings until that package is committed and the lock is updated.
