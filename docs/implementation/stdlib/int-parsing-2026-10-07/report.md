# Int parsing checkpoint

Starting compiler: 559695c. The library moves from
d6ab1527e1711eb3c712f5d7bc70e5079bb8eaee to
1ad9a1a9fba01f35c694d877fd4890f8d2e46cb9, with fingerprint fnv1a64-v1:f728b54c2aadd888. This compiler
commit updates its package lock and acceptance evidence; no Rust source changes.

## Owner and source fidelity

Data.Int's private fromStringAsImpl slot delegates to PSRS.Int.Parse. Data.Int
owns unwrapping its opaque Radix. Official exports, pure public wrappers,
instances and rank-N constructor signatures remain unchanged. The library
verifies the entire module against the clean purescript-integers v6.0.0 checkout
at 54d712b25c594833083d15dc9ff2418eb9c52822, allowing exactly this delegate
and the previous toNumber native binding.

The target library implements strict ASCII digit grammar over UTF-8 bytes.
A nonpositive accumulator supports MIN without wrapping; checks before multiply
and subtract reject overflow. The threshold uses truncating intQuot, rather
than the compiler's floor intDiv operation. There is no new intrinsic or
compiler exception for library names.

The same quotient-owner error existed in PSRS.Int: its Euclidean sign adjustment
was applied to an already rounded intDiv result. The old pinned library's public
`div (-7) 3 == -3` fixture compiled but returned 1. The helper now binds intQuot
and retains its existing raw remainder, zero handling and single library-owned
Euclidean correction. Backend primitive semantics are unchanged.

## Verification

- 836 pinned official parsing FFI observations cover all bases 2 through 36,
  both i32 boundaries and adjacent rejection, signs, leading zeros, upper/lower
  ASCII letters, invalid digits, prefix-looking input, whitespace, non-ASCII,
  embedded NUL and long overflow strings. Public fromString/fromStringAs and
  named radix constants execute. Six public radix-factory checks and four local
  rank-N builder checks are recorded separately: 846 checks in total.
- 209 public degree/div/mod observations agree with pinned Prelude FFI for
  representable i32 results, including zero divisors, negative divisors and
  boundary values. MIN / -1 is recorded as an unrepresentable JS result;
  target signed division still traps, so this is not a raw-JS equality claim
  for that input.
- Both generated programs return 42 with empty stdout and stderr. Development
  reports show the package unchanged during each run. run.json additionally
  records direct builds through stdlib.lock.json with PSRS_STDLIB_ROOT unset,
  compiler/input/Wasm digests and actual Wasmtime output.
- Node tooling: 10 passed, zero skips. Mandatory Wasmtime driver scalar tests:
  16 passed. Development-root and normal locked trusted-order loader tests:
  one passed each; Prelude remains first.
- Source audit: 226 modules, 41 packages; 171 identical, 35 modified, 20 target
  additions, zero missing upstream modules and zero direct recursive foreign
  placeholders. These aggregate counts do not approve other source differences.
- The same minimal public Int.fromString probe changes from explicit P8 missing
  fromStringAsImpl support to Passed through the locked package. diagnosis.json
  retains both snapshots; differing fingerprints preclude a compatible
  diagnose --compare claim.
- Formatting and diff checks pass. Full workspace/clippy were not repeated for
  this library-only slice. The preceding compiler checkpoint recorded 1664 pass,
  3 established baseline assertion failures and 5 ignored tests, with strict
  clippy passing. Those results do not establish full verification of this pin.

No full import cohort or suite scoreboard is remeasured. This checkpoint does
not establish full Data.Int or standard-library support. Formatting integers
and other remaining foreign slots are outside its scope.

A fresh public `Int.toStringAs Int.hexadecimal 42` probe reaches P8 library
linking with explicit missing Data.Int.toStringAs support. next-blocker.json
records the new pinned package and this continuation point.
