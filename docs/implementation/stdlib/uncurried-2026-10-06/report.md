# Uncurried functions checkpoint

The compiler gains no `Fn` intrinsic. `Data.Function.Uncurried` keeps its
official signatures and exports; `Fn1` stays the synonym `a -> b`, and `Fn0` and
`Fn2` through `Fn10` become abstract newtypes over the corresponding curried
function. `mkFn{N}` and `runFn{N}` adapt currying only, and keeping each type
abstract preserves rank-2 arguments. This unblocks `Data.Array`, whose foreign
signatures use `Fn2` and `Fn3`. The independent `psrs-stdlib` revision is
`b405dddf67a902387dda316cdf331f2cc82db5ce` with content fingerprint
`fnv1a64-v1:03a4b91b0b29e0ec`.

Validation:

- Mandatory Wasmtime driver regressions passed, including the new
  `library_uncurried_matches_pinned_official_observations` and the earlier apply,
  bind, extend, ST and primitive checks.
- Official JS observations: the pinned `Data/Function/Uncurried.js` generates
  eleven cases with eleven observable checks covering the
  `runFn{N} (mkFn{N} f)` round trip for `N = 0` and `N = 2..10` plus a
  first-class `Fn2` passed through an ordinary function. The mandatory runner
  executes the Prelude-free fixture under Wasmtime with exit 42 and empty output.
- Library-owned Node tooling regressions: 8 passed.
- Complete pinned audit: 41 packages, 217 modules, 189 exact, 17 modified and
  11 explicitly recorded target additions; no absent official module and no
  detected exact direct same-argument recursion.
- Formatting and strict workspace clippy with warnings denied passed.

The full reproducer still fails. Unsupported-library P8 reports fell from 199 to
179 and the first is still `Data.Array.fromFoldableImpl`; no
`Data.Function.Uncurried` foreign value remains. Package content changed from
`fnv1a64-v1:5b51b652c77ed0b7` to `fnv1a64-v1:03a4b91b0b29e0ec`. Remaining work in
this chain includes the `Data.Array` algorithms, `Data.Array.NonEmpty.Internal`,
and the mutable `Data.Array.ST` group. No full workspace tests or full scoreboard
were run. No push, PR or issue operation was performed.
