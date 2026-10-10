# Scalar String length and slicing checkpoint

The compiler prerequisite topics are committed as 138da4e (canonical qualified
scopes), cdeefed (guard continuations and Bounded pin), and 4aa5b51 (recursive
product coverage). The String library slice changes no Rust compiler mechanism.

The independent library pin is c8020c00e227f256c0a365e22d5ebcbd1601607a,
content fingerprint fnv1a64-v1:6d4f0e0ccbe37bc0. Data.String.CodeUnits keeps
all official exports, signatures and pure declarations. Its five foreign slots
length, take, drop, slice and splitAt delegate to PSRS.String. The library scans
validated canonical UTF-8 at scalar boundaries, copies through the existing
PSRS.Array.sliceImpl, and validates output with bytesToString. It adds no
whole-function intrinsic and keeps mutation primitives private to the array
storage owner. Prelude remains first in trusted loading.

DEC-16 makes these source indices Unicode scalar indices, not bytes or UTF-16
units. The pinned official FFI executes against a one-BMP-unit-per-scalar
projection, then output maps back to original scalars. Raw UTF-16 results and
the FFI digest are recorded separately in the library's observations. This is
an explicit target difference, not raw-JS equality on supplementary text.

Validation:

- 448 projected-FFI/runtime observations return 42 under Wasmtime 49.0.2 with
  empty stdout/stderr. Cases cover NUL, combining characters, all UTF-8 width
  transitions, supplementary scalars, U+10FFFF, empty/reversed/negative ranges,
  oversized indices and both signed i32 extrema. The unchanged takeRight and
  dropRight wrappers also execute. The final package run is in run.json.
- The same String-length probe changes from explicit P8 missing binding to
  compile acceptance (4794 ms). Library fingerprints differ, so this is not
  presented as a compatible diagnose --compare result. diagnosis.json records
  both observations.
- The source audit covers 225 modules in 41 pinned packages: 171 identical,
  35 modified and 19 platform additions, no missing upstream module and no
  direct-self-recursion replacement. The exact five-slot transformation is
  checked by the oracle generator; aggregate audit categories alone do not
  approve differences.
- Library Node tooling: 8 passed. Development-root and locked-root trusted-order
  loader checks pass. Mandatory Wasmtime driver string regressions: 16 passed.
- Formatting and diff checks pass. No Rust source changes were made in this
  slice, so the full workspace/clippy checks from the prerequisite checkpoint
  were not repeated. That checkpoint still records three independently
  reproduced driver-library baseline failures.

The finite-expression stack overflow recorded for this checkpoint is repaired
by the [compiler follow-up](../deep-expression-2026-10-07/report.md).
The original ungrouped 448-observation String fixture now compiles and returns
42 with empty stdout/stderr under the same library fingerprint. The library
generator retains its grouped fixture; no library source or oracle result was
changed to accommodate the compiler repair.

Character indexing, predicate traversal, searching and other String slots
remain explicitly unsupported. This slice does not claim full String or
stdlib API/runtime closure. The later compiler follow-up separately remeasures
the full 211-import reproducer under this library pin.
