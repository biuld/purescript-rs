# Library array algorithms and checked storage checkpoint

The compiler no longer owns whole-function array application or binding kernels.
The independent `psrs-stdlib` revision is `23ba4b9de6ba310a8c77e4c6a1e5cd56d68c528e`. It owns ordinary
PureScript `PSRS.Array` implementations and explicit target provenance. Official
`Control.Apply` and `Control.Bind` only add an import and replace their foreign
slots with target aliases; their public signatures and other pure code remain.

The compiler adds initialized `arrayFill` and unsafe in-place `arrayWrite`.
Core validates element identities, CC carries checked representations and values,
MIR validates length/initializer/storage/destination and lowers initialized
allocation directly to Wasm GC `array.new`. Writes use existing array stores.
No callback or traversal algorithm is hidden inside these primitives. The old
`ArrayApply` registry/CC/MIR operation and callback invoker were removed.

A general primitive linking defect was exposed by mutation: a polymorphic
foreign wrapper adapted its concrete input by copying the array, so a write
changed the copy. Mandatory Core occurrence elaboration now expands every
validated primitive global using its checked occurrence type before ABI erasure,
including first-class and partial uses. The common collision-free Core local
allocator is shared with optimization. Complete candidate verification and
rollback remain required; unused incorrect primitive signatures still fail.

Validation:

- Mandatory Wasmtime driver primitive/library regressions: 16 passed. They
  include first-class/partial bindings, array aliases and unused writes,
  initialized references/records/closures, invalid lengths/indices and element
  relationships, captures, returned functions, empty arrays and product overflow.
- Official JS observations: 8 apply cases/29 checks and 9 bind cases/37 checks,
  both exit 42 with empty output. Bind includes callback count and immediate
  snapshots of a shared mutable array. The 46-case scalar oracle also passed.
  The independent library owns the generators, observations and runtime reports
  in `docs/evidence/source-array-kernels-2026-10-06/`.
- Malformed MIR filled-array test: passed; wrong initializer or noninteger
  length is rejected, and a valid initialized allocation is accepted.
- Transactional primitive linking tests: 3 passed.
- Core optimization regressions after sharing local allocation: 24 passed.
- Intrinsic descriptor arity test: passed.
- Locked trusted-library loader regression: passed; Prelude remains first.
- Let-constraint regressions: 14 passed.
- Library-owned Node tooling regressions: 8 passed.
- CLI rebuild, formatting, and workspace clippy with warnings denied passed.
- Complete pinned audit: 41 packages, 216 modules, 193 exact, 13 modified and
  10 explicitly recorded target additions; no absent official module and no
  detected exact direct same-argument recursion. The audit is comparison
  evidence, not blanket approval of every adaptation.

The unchanged full reproducer still fails. Unsupported-library P8 reports fell
from 232 to 231 and the first is `Control.Extend.arrayExtend`. The snapshot
`diagnose.json` records the 38.937-second development run and complete diagnostics.
Package content changed from `fnv1a64-v1:aca9437f2fcdd5a8` to
`fnv1a64-v1:9c725bfe5140502e`, so these are explicit migration measurements,
not an unchanged-cohort `diagnose --compare` result or a suite scoreboard update.

Remaining work includes full stdlib compile/runtime/FFI acceptance, unsupported
bindings and layouts, general polymorphic array identity/mutation interactions,
and independent CI package acquisition. Physical array mapping can still copy
across ordinary polymorphic function boundaries; these private builder tests
prove no broader alias guarantee. No full workspace tests or full scoreboard
were run. No push, PR or issue operation was performed.
