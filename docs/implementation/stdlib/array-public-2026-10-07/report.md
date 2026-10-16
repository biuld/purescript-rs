# Public `Data.Array` and mutable `Data.Array.ST` checkpoint

This slice makes the public `Data.Array` API executable end to end and lands the
mutable `Data.Array.ST`/`Data.Array.ST.Partial` group. It extends the
[array operations](../array-operations-2026-10-06/report.md) and
[ST](../st-2026-10-06/report.md) checkpoints. The independent
`psrs-stdlib` revision is `332c69b16f240254f74375b7babed7f752f7361a` with content
fingerprint `fnv1a64-v1:f3b995062d3662ba`.

The mutable array representation changes from an opaque foreign data type to
`newtype STArray h a = STArray (STRef h (Array a))`, an alias of the shared
`Control.Monad.ST.Internal` cell. Every `Data.Array.ST` foreign slot is replaced
by a source adapter over `PSRS.Array.Mutable`, which owns the fixed-length array
algorithms over the private `arrayFill`, `arrayWrite`, `arrayIndex`,
`arrayLength`, `arrayAppend` and integer primitives. `Data.Array.ST` keeps its
official signatures, exports and pure code; the rank-2 `shift`/`pop` observers
are wrapped through `mkSTFnN` and an `unsafeCoerce` bridge.

Several compiler prerequisites are part of this slice rather than the library:

- **Executable library reachability.** `Core::prune_unreachable` now drops a
  static library import and its checked signature unless the executable
  dependency graph reaches it, while explicit primitive and WIT declarations keep
  their source-wide protocol validation. This is the contract recorded in
  [D-17](../../../design/D-17-stdlib-and-conformance-boundaries.md).
- **`Unsafe.Coerce` as a checked primitive.** `Unsafe.Coerce` is a
  compiler-provided module now; `unsafeCoerce` lowers to a checked
  `RepresentationCast` instead of a source body that cannot compile.
- **Superclass dictionary thunks.** A superclass field is a `Unit -> Dict`
  thunk, forced at selection, so mutually referring instance values no longer
  recurse before a method runs.
- **Closed local row instantiations.** Checked row residuals are retained as
  instantiation evidence even when they have no single type-arena node, and a
  finite closed use of a nonrecursive row-polymorphic local lambda is
  materialized before CC layout. The reusable type substitution moved to
  `psrs-core::instantiation`.
- **Aggregate protocols for bare polymorphic slots.** The array and record
  layout owners register canonical erased-element array and erased-field product
  protocols before conversion planning; the array protocol is registered only
  when an array layout exists. Newtype callable fields get their own protocol.
- **Partial authorization.** A checked report-only `Partial` scope adds a
  marked trap fallback that survives erasure and contributes coverage instead of
  hiding redundant source alternatives.
- **Qualified symbolic operators.** Contiguous module qualifiers followed by a
  symbolic name resolve, so `Data.Array.(..)`, `A.:`, `A.!!` and
  `Foo.Bar.-#-` parse, resolve and lower.

Validation:

- Mandatory Wasmtime driver regressions for the touched areas pass:
  `library_array_callbacks_match_pinned_official_observations` (the new
  sixteen-case/69-value callback fixture), the earlier apply/bind/extend/ST/
  uncurried/primitive array checks, `declaration_calls`, `library_foreign`,
  `cc_ir_audit` and `generic_aggregate_audit`.
- Public API oracle: `conformance/array-public-cases.mjs` covers all 93 public
  `Data.Array` exports with 105 cases and 396 checks. The library-owned
  `array-public-oracle` command pinned all 41 upstream packages, evaluated the
  pinned official JavaScript, and wrote `Main.purs`. The mandatory runner
  compiled it on the target and executed the artifact under Wasmtime with exit
  42 and empty stdout/stderr. `public-run.json` records the binary/source/package
  hashes and `observations.json` records the coverage.
- Library-owned Node tooling regressions: 8 passed.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D
  warnings` pass.

The slice introduces no new driver-library failure: the driver suite returns to
its pre-slice set and the 174 cases this work unblocks stay green. The
`Data.Array.ST.Partial` missing final newline is repaired.

Two limits are recorded rather than hidden. Open runtime rows remain an explicit
unsupported layout, so the same machinery does not define an open-row ABI. The
`Data.Show` foreign slots still have no target binding, so programs that reach
`show` fail at P8 library linking; that gap predates this slice and is unchanged.
The full reproducer was not re-measured, so no unsupported-library count is
published here. No push, PR or issue operation was performed.
