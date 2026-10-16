# Checked Number-to-Int checkpoint

Starting compiler: deb8304, with a clean worktree. The independent library moves
from 02929696a90db4a8277151aab9a8c0a9978f8aa3 to 3e3736dec40e873da6930d4864e7db4b525570ce,
content fingerprint fnv1a64-v1:5e7c1b9bf50e4915. This compiler commit changes
its lock and conformance evidence; no Rust source changes.

## Owner and source fidelity

Data.Int.fromNumber remains its official pure wrapper. The private fromNumberImpl
foreign slot delegates to PSRS.Int.Number with its full rank-N constructor
signature. The source verifier checks the entire official purescript-integers
v6.0.0 module at 54d712b25c594833083d15dc9ff2418eb9c52822. It permits
exactly four typed foreign-slot adaptations and their target imports: native
toNumber, parsing, formatting and checked Number conversion. All other pure
functions, exports, signatures, classes, instances and opaque Radix remain.

The target library widens the existing total saturating numberToInt result and
checks exact Number equality before calling the success builder. Every i32 is
exactly representable in f64. Fractions and overflow fail equality; infinities
differ from finite bounds and NaN is unequal to every widened result. This
proves success exactly for integral, in-range inputs without relying on a
trapping conversion. No new intrinsic or compiler name exception is added.

Both Number zero signs are accepted. Int has a single signed-i32 zero, so
widening its result canonicalizes negative zero to positive Number zero. The
official JS implementation can preserve negative Number zero inside its Int;
observations retain that representation difference explicitly rather than
claiming identical signed-zero bits.

## Verification

- 94 public fromNumber observations agree with the actual pinned Data.Int.js
  FFI as Maybe Int values. Cases cover both i32 endpoints, immediately adjacent
  IEEE-754 values on either side, fractions, subnormals, signed zeros, huge finite
  values, unsigned and 53-bit boundaries, NaN and positive/negative infinity.
- Eleven public toNumber/fromNumber round trips and six local rank-N builder
  checks are recorded separately: 111 checks total. Development and direct
  locked builds return 42 with empty stdout/stderr. run.json records the direct
  build with PSRS_STDLIB_ROOT unset, input/compiler/Wasm digests and actual
  Wasmtime result. The development runner records immutable package content.
- The preceding 1734 public formatting/parse-round-trip checks return 42 with
  empty output against the same package fingerprint. Library observations and
  runtime evidence live under docs/evidence/int-number/.
- Node tooling: 10 passed, zero skips. Mandatory Wasmtime driver scalar tests:
  16 passed. Development and locked trusted-order loader tests: one passed each;
  Prelude remains first.
- Source audit: 228 modules in 41 packages; 171 identical, 35 modified, 22 target
  additions, zero missing upstream modules and zero direct recursive foreign
  placeholders. Aggregate categories do not approve other source differences.
- The same public fromNumber 42.0 probe changes from explicit P8 missing
  Data.Int.fromNumberImpl support to Passed through the locked package.
  diagnosis.json retains both snapshots; differing fingerprints preclude a
  compatible diagnose --compare result.
- Formatting and diff checks pass. Full workspace/clippy were not rerun for this
  library-only slice. The preceding Rust checkpoint recorded 1664 passed,
  3 established baseline assertion failures and 5 ignored, with strict clippy
  passing; that historical run does not validate the entire new library pin.

No full import cohort or official suite scoreboard is remeasured. This checkpoint
does not establish all Data.Int APIs; its rounding/clamping wrappers require
additional Data.Number implementations and independent behavior evidence.

A fresh public `Int.trunc 42.9` probe first reaches explicit P8 missing
Data.Number.isFinite support. next-blocker.json records this continuation
point under the new locked package.
