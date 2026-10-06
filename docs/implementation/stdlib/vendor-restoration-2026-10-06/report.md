# Standard-library restoration checkpoint

This checkpoint describes the uncommitted restoration on top of revision
`67369ba`, on `stdlib/vendor-core-libraries`. It does not supersede the
[historical audit](../vendor-audit-2026-10-06/report.md), which records the
defects in that revision. The governing policy is
[standard-library source fidelity](../../../workflow/stdlib-vendoring.md).

## Source evidence

The [inventory](inventory.json) pins all 41 upstream packages by release tag,
commit, and per-module SHA-256. All 206 official modules are now present:
201 are byte-identical and five have target adaptations. The nine additional
modules are the existing WASI integration. No exact top-level same-argument
self-recursion remains; the detector previously found 200 such placeholders.
This syntactic check is not a general termination or semantic-equivalence proof.

The four restored modules are `Effect.Class`, `Effect.Class.Console`,
`Effect.Uncurried`, and `Effect.Unsafe`. The official Tuple instances, Show
surface, assertion wrappers, console Show wrappers, pure combinators, and
instance method definitions have been restored. Original vendored source
length is preserved under the explicitly approved exception to the maintained
source-file limit.

See [the exact remaining diff](official-vs-vendored.diff) and
[the module table](modules.md). An inventory difference is not approval of
an adaptation or proof that its implementation works.

## Target adaptations and remaining obligations

| Module | Target reason and owner | Preserved contract | Unverified obligations |
| --- | --- | --- | --- |
| `Prelude` | Existing state-token Effect operations and command entry, governed by the effects design | Ordinary official re-exports; Effect methods use the official combinators; representational role retained | Source ownership of Effect is relocated; execute sequencing and callback cases under mandatory Wasmtime |
| `Data.Unit` | Compiler-owned builtin Unit representation | Public Unit and unit are re-exported | Verify builtin ownership against the official public role and instance contract |
| `Effect` | Source wrappers around the target state-token operations | Official exports, loop signatures, Semigroup and Monoid bodies; core instances remain in Prelude | Execute all four loops, zero iterations, order, boundary inputs, and termination; source recursion may differ from the upstream iterative implementation in resource use |
| `Effect.Console` | WASI streams replace JavaScript console methods | All public values and official Show wrappers restored | Verify stream routing and output; time, timeLog, timeEnd, and clear retain foreign declarations without implementations |
| `Test.Assert` | WASI stderr plus the existing guest-trap failure protocol | All ten public values and ordinary comparisons/message construction restored | Execute successful and failed assertions; checkThrows and catch-based assertion behavior remain unsupported |

The Effect implementation owner and protocol are described in
[effects](../../../design/backend/fp/effects.md). Unit ownership is described
in [scalars and primitives](../../../design/backend/fp/scalars-and-primitives.md).
Unicode target differences remain governed by DEC-16; no numeric formatting
approximation or pure API deletion is justified by UTF-8 storage.

These adaptations are candidate implementations, not completed runtime
acceptance. In particular, retaining the `checkThrows` declaration preserves
the public source surface without pretending that guest traps can be caught.

## Foreign declarations and compiler contracts

Ordinary `foreign import` now retains its absence of an explicit WIT binding in
AST and its declaring module in HIR. Source declarations enter the ordinary
value namespace before fixity resolution. Imports and re-exports retain their
declaration symbol and signature and take precedence over bootstrap intrinsics.
THIR and Core require checked external signatures for ordinary library imports
as well as WIT imports. Foreign signatures with class constraints remain
rejected, matching the official compiler's restriction.

P8 reports a missing target implementation explicitly, including declaration
ownership. No recursive source body or fabricated WIT import replaces it.
All 275 retained ordinary foreign declarations still need target implementations
and behavior evidence. The [binding inventory](foreign-bindings.json) links
every declaration to its pinned official source; the
[module summary](foreign-bindings.md) distinguishes the complete inventory from
the reproducer's import closure.

Future target implementations must use an explicit binding contract, validate
the checked source signature, and retain producer/consumer evidence until the
owning lowering discharges it. Matching a spelling or guessing an arity cannot
establish representation compatibility. Scalar operations, polymorphic array
callbacks, mutable ST/Ref state, lazy values, uncurried functions, regexes,
number rendering, and failure behavior require their own observable tests.

## Validation

The pre-restoration import-only reproducer compiled in 21,707 ms, with the
recursive placeholders still present. After restoration and a fresh offline
CLI build, the same `/tmp/psrs-stdlib-all.purs` failed in 26,669 ms with
253 `P8 library linking` diagnostics and no earlier diagnostics. A subsequent
rebuild after completing THIR signature validation produced the same 253
diagnostics in 21,996 ms. The first is
`Control.Apply.arrayApply`. The snapshot is `/tmp/psrs-stdlib-all.json`; its
complete input set is recorded, with trusted-library fingerprint
`fnv1a64:3f3081e2d1ea1032`. Source inputs intentionally changed, so this is
restoration evidence rather than a compatible-input compiler-only comparison.

Focused validation:

- `cargo test -p psrs-driver --lib let_constraints --offline`: 14 passed.
- Six passing library foreign tests cover checked evidence, source shadowing, imports,
  fixities, forbidden constraints, and explicit missing-implementation errors.
- `PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver
  --test upstream differential_library_foreign --offline`: passed against
  official purs 0.15.16, including a same-name bootstrap collision.
- The trusted-order loader test passed. This validates loading/checking, not
  target implementation or runtime behavior.
- `cargo test -p psrs-core --lib external_types --offline`: seven passed.
- THIR checked-external tests: six passed, including missing library evidence.
  Backend binding projection tests: four passed. The existing class-constrained
  WIT rejection test also passed with its original diagnostic.
- `cargo fmt --all --check` and strict workspace clippy with all targets passed.

The filtered resolver command selected zero tests and supplies no independent
resolver coverage; the driver and official comparison exercise that boundary.
No workspace test suite, full runtime scoreboard, or full FFI behavior suite was
run. No D-04 or README measurement was changed. The restored stdlib does **not**
compile fully and does **not** have full runtime acceptance.

## Reproduce the source comparison

```sh
python3 docs/workflow/tools/audit-stdlib-vendor.py \
  --vendor stdlib/lib \
  --upstream /tmp/ps-pkgs \
  --upstream /tmp/purescript-prelude \
  --upstream /tmp/psrs-stdlib-audit-20261006/upstream \
  --out /tmp/psrs-stdlib-fidelity-current
```

Those directories must contain the exact clean tagged checkouts recorded in
the inventory. The comparison tool does not fetch dependencies or approve
changes. Preserve the historical audit separately from new measurements.
