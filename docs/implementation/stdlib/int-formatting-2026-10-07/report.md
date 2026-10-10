# Int formatting checkpoint

Starting compiler: dab23d7, with a clean worktree. The independent library moves
from 1ad9a1a9fba01f35c694d877fd4890f8d2e46cb9 to 02929696a90db4a8277151aab9a8c0a9978f8aa3,
content fingerprint fnv1a64-v1:83cdad11334226e0. This compiler commit changes
its lock and conformance documentation; no Rust source changes.

## Owner and source fidelity

Data.Int unwraps its opaque checked Radix and delegates the toStringAs foreign
slot to PSRS.Int.Format. Its signature, exports and all official pure declarations
remain unchanged. The entire module is checked against clean pinned
purescript-integers v6.0.0 at 54d712b25c594833083d15dc9ff2418eb9c52822;
the verifier permits exactly the native toNumber binding, parsing delegate and
formatting delegate with their target imports.

The library owns radix digit extraction over raw truncating intQuot and signed
remainder. Keeping the accumulator nonpositive supports MIN without overflow.
A checked base in 2..36 proves remainder digits in 0..35, all output bytes ASCII
and termination within 32 digits. Zero produces one digit and negatives one
leading minus. Existing array append and checked bytesToString primitives
assemble canonical UTF-8; no new compiler intrinsic or library-name exception
is added. The previous PSRS.Show decimal digit implementation now calls the
same intBytes operation with fixed base 10, including numeric control escapes.

## Verification

- 857 public toStringAs observations agree byte-for-byte with the actual pinned
  official Data.Int.js FFI across all bases 2..36, MIN/MAX and adjacent values,
  zero, signs, digit transitions and powers of each base.
- 857 public parse/format round trips, 15 official Data.Show.js showInt comparisons
  and 5 named-radix checks bring the generated fixture to 1734 checks. Each
  group is bounded to 24 conjunction terms to limit fixture nesting; the
  separate deep-expression compiler regression remains its own evidence.
- The generated program returns 42 with empty stdout/stderr in both development
  and locked-package builds. run.json records direct build with PSRS_STDLIB_ROOT
  unset, lock/input/compiler/Wasm digests and actual Wasmtime execution. The
  development runner confirms the package unchanged during execution.
- The preceding 846 parsing checks return 42 with empty output against the same
  package fingerprint. Observation and runtime data live in the library under
  docs/evidence/int-formatting/.
- Node tooling: 10 passed, zero skips. Mandatory Wasmtime Show driver tests:
  6 passed, including number/aggregate boundaries, control escaping, retained
  strings through allocation and memory growth. Development and locked trusted
  loader tests: one passed each; Prelude remains first.
- Source audit: 227 modules in 41 packages; 171 identical, 35 modified, 21 target
  additions, zero absent upstream modules and zero direct recursive foreign
  placeholders. Exact source verification and behavior evidence approve this
  slot; aggregate inventory categories do not approve other changes.
- The same public hexadecimal formatting probe changes from explicit P8 missing
  Data.Int.toStringAs implementation to Passed through the locked loader.
  diagnosis.json retains both snapshots; different package fingerprints preclude
  a compatible diagnose --compare claim.
- Formatting and diff checks pass. Full workspace/clippy were not rerun for this
  library-only slice. The preceding Rust checkpoint recorded 1664 passed,
  3 established baseline assertion failures and 5 ignored, with strict clippy
  passing. That earlier run does not validate this entire new library pin.

No import cohort or official suite scoreboard is remeasured. This checkpoint
establishes public integer formatting behavior for the tested values, not full
Data.Int or standard-library support.

A fresh public `Int.fromNumber 42.0` probe reaches P8 library linking with
explicit missing Data.Int.fromNumberImpl support. next-blocker.json records
this continuation point under the new locked package.
