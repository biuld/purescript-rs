# Scalar String character checkpoint

Starting compiler: fafda68, with a clean worktree. The independent library moves
from c8020c00e227f256c0a365e22d5ebcbd1601607a to
d6ab1527e1711eb3c712f5d7bc70e5079bb8eaee, content fingerprint
fnv1a64-v1:17383a9270e6c69e. This compiler commit updates its lock, the DEC-16
implementation status and the conformance workflow; no Rust source changes.

## Owner and source fidelity

Data.String.CodeUnits.charAt and toChar retain their official pure wrappers,
exports and rank-N constructor signatures. The _charAt and _toChar foreign
slots delegate to PSRS.String.charAtImpl and toCharImpl. The library owns scalar
position traversal and private canonical UTF-8 decoding over existing checked
primitives. At each decoder call, the validated String producer and scalar
boundary scan establish bounds and prove a scalar result before intToChar's
identity representation conversion. No compiler name exception or new primitive
is added.

The shared library source verifier compares the entire module against the
clean pinned purescript-strings v6.0.1 source at
3d3e2f7197d4f7aacb15e854ee9a645489555fff, permitting exactly the seven foreign
delegates and their target import. All official pure declarations are preserved.
The source audit covers 225 modules in 41 packages: 171 identical, 35 modified,
19 target additions, zero missing upstream modules and zero direct recursive
foreign placeholders. Aggregate audit categories do not approve source changes;
the exact transformation and DEC-16 behavior evidence govern these two slots.

## Verification

- 269 public charAt/toChar observations execute the pinned official FFI on a
  one-BMP-unit-per-scalar projection, then map Maybe results back to the original
  scalar. Raw JS observations are retained separately, including surrogate
  halves and raw _toChar rejection of supplementary text. This explicitly
  follows DEC-16 and does not claim raw-JS equality on supplementary strings.
- 6 target-helper checks exercise local rank-N builders, including builders
  that discard successful values. The generated 275-term conjunction compiles
  and returns 42 under Wasmtime 49.0.2 with empty stdout/stderr. The final direct
  build unsets PSRS_STDLIB_ROOT and consumes stdlib.lock.json; run.json records
  the lock, input/compiler/Wasm digests and actual runtime result.
- The preceding 448 length/slicing observations also return 42 against the same
  package fingerprint. The library retains all observation data and both runtime
  reports under docs/evidence/string-characters/.
- Node tooling: 9 passed, no skips. Both development-root and normal locked-root
  trusted-order loader tests pass; Prelude remains first. Mandatory Wasmtime
  String driver regressions: 16 passed.
- The same minimal supplementary-charAt probe changes from explicit P8 missing
  _charAt support to compilation success (4820 ms) through the locked loader.
  diagnosis.json records both snapshots. The library fingerprints differ, so
  these are not presented as a compatible diagnose --compare result.
- Formatting and diff checks pass. Full workspace/clippy were not repeated for
  this library-only slice. The preceding compiler checkpoint records 1664 pass,
  3 independently established baseline failures and 5 ignored tests, with strict
  clippy passing; those results do not claim full verification of the new pin.

The 211-import fixture is not remeasured for this library pin. Unsafe character
indexing, character arrays, predicate traversal, searching and other foreign
slots are outside this checkpoint. A fresh Int.fromString probe still reaches
the explicit P8 missing Data.Int.fromStringAsImpl implementation. This slice
does not establish full String or standard-library API/runtime acceptance.
