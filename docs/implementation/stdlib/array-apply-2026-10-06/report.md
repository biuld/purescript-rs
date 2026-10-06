# Polymorphic array application checkpoint

The target implements `Control.Apply.arrayApply` through the explicit registry
binding `psrs:intrinsic#arrayApply`. The independent library revision is
`b367cd7cc4b2e9f5933bffb9938a1cdbe221fd8f`. Its only additional official-source
difference is the foreign binding string; the signature and pure declarations
remain unchanged. The upstream remains pinned prelude v6.0.1 at
`f4cad0ae8106185c9ab407f43cf9abf05c256af4`.

The prerequisite is a general primitive foreign-function link contract: leading
quantifier identities move into the generated declaration's scope, while the
body retains its checked type relationships. Existing array/byte operations
share their Core verification and backend implementation. Unsupported categories
still fail, and linking retains its complete rollback contract.

Array application carries its callback signature, array representations, and
invoker through CC. The invoker uses the common application/partial-call path,
including returned functions. The full CC verifier checks its actual ABI and
reachability retains it. MIR allocates once and emits function-major nested
loops, caching each function for its entire inner traversal. It preserves both
inputs and traps on unrepresentable lengths before allocation or callbacks.

Evidence:

- Driver primitive foreign tests: 13 passed with mandatory Wasmtime. They cover
  captures, callback/result order, Int/Number/String/record/nested-array values,
  empty arrays, returned curried functions through both entry points, invalid
  signatures, and an overflowing `2^32` result length that must trap.
- Official JS oracle: eight cases and 29 value checks; Wasmtime exit 42 with
  empty stdout and stderr. `run.json` records binary/source/package/Wasm hashes
  and commands; `observations.json` records the pinned official observations.
- CC invalid-invoker test: one passed; missing helpers and incorrect result ABIs
  are rejected by the complete check.
- Primitive transactional/link-identity tests: three passed.
- Core scheme scope/malformed-spine tests: two passed.
- Intrinsic descriptor/arity test: one passed.
- Let-constraint regressions: 14 passed.
- CLI build, formatting, and workspace clippy with warnings denied passed.
- Full source audit: 41 packages, 215 modules, 194 exact, 12 modified, nine
  platform additions, no missing upstream modules, no detected direct
  same-argument self-recursions. Audit counts do not approve every adaptation.

The full stdlib reproducer remains failed. P8 missing-library-implementation
reports decreased from 233 to 232; the first is now `Control.Bind.arrayBind`.
The input file is unchanged, but package content and its diagnosis cohort
fingerprint changed deliberately; these are migration measurements, not an
unchanged-cohort comparison or an official-suite scoreboard update.

No full workspace tests or full scoreboard were run. Whole-library compile and
runtime/FFI acceptance, remaining implementations, and independent CI package
acquisition remain open. No push, PR, or issue operation was performed.
