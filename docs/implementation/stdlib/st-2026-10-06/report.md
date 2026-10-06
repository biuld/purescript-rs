# ST computations and references checkpoint

The compiler gains no `ST` intrinsic. `Control.Monad.ST.Internal` and
`Control.Monad.ST.Uncurried` keep their official signatures, exports, classes,
instances and other pure code; their foreign implementation slots delegate to
the ordinary PureScript target module `PSRS.ST`. The independent `psrs-stdlib`
revision is `567c00dd9d4c7dc53814f12aaf2aaf6e5b4f34d9` with content fingerprint
`fnv1a64-v1:5b51b652c77ed0b7`.

`PSRS.ST` represents an `ST` action as `Action r a`, a suspended `Unit -> a`
thunk, and an `ST` reference as `Cell r a`, a fresh one-element mutable array
over the existing private `arrayFill`, `arrayWrite` and `arrayIndex` primitives.
`Control.Monad.ST.Internal` declares `ST`/`STRef` as newtypes over those and
keeps every class instance; the instances cannot move to `PSRS.ST` because this
compiler does not resolve an instance whose type comes from another module
(orphan). Each operation is therefore a thin adapter, and the exercised
algorithm lives in the Prelude-free target module. `STFn{N}` stays abstract as a
newtype over the curried action so `Data.Array.ST`'s rank-2 uses typecheck.

Validation:

- Mandatory Wasmtime driver regressions: 18 passed, including the new
  `library_st_matches_pinned_official_observations` and the earlier apply, bind,
  extend and primitive checks.
- Official JS observations: the pinned `Control/Monad/ST/Internal.js` generates
  ten ST cases with ten observable checks covering `pure`/`map`/`bind`, fresh
  distinct cells, read, write, modify, `while`, `for`, `foreach`, and empty
  inputs that must not invoke their callbacks. The mandatory runner executes the
  Prelude-free target fixture under Wasmtime with exit 42 and empty output.
  Observations and the runtime report are written to a local, git-ignored
  directory.
- Library-owned Node tooling regressions: 8 passed.
- Complete pinned audit: 41 packages, 217 modules, 190 exact, 16 modified and
  11 explicitly recorded target additions; no absent official module and no
  detected exact direct same-argument recursion. The audit is comparison
  evidence, not blanket approval of every adaptation.
- Formatting and strict workspace clippy with warnings denied passed.

The full reproducer still fails. Unsupported-library P8 reports fell from 230 to
199 and the first is now `Data.Array.fromFoldableImpl`; no
`Control.Monad.ST.Internal` or `Control.Monad.ST.Uncurried` foreign value
remains. The snapshot `diagnose.json` records the development run and complete
diagnostics. Package content changed from `fnv1a64-v1:e4195415d1de337c` to
`fnv1a64-v1:5b51b652c77ed0b7`, so these are explicit migration measurements, not
an unchanged-cohort `diagnose --compare` result or a suite scoreboard update.

Two limits are recorded rather than hidden. First, the target's `ST r a` is a
thunk over `Unit` while the compiler's `Effect a` is a closure over a scalar
state token, so `Control.Monad.ST.Global.toEffect`'s `unsafeCoerce` is linked
but not sound on this target and is not exercised. Second, physical array
copying across general polymorphic function boundaries remains a compiler
obligation; the cell newtypes are parameterized so the exercised paths preserve
identity, which does not prove a broader alias guarantee. Runtime execution of
the official wrapper modules is not separately observable until the rest of the
Prelude closure links; the tested target module has no Prelude dependency. No
full workspace tests or full scoreboard were run. No push, PR or issue operation
was performed.
