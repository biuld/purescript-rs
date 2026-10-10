# Array operations checkpoint

`Data.Array` and `Data.Array.NonEmpty.Internal` keep their official signatures,
exports and other pure code; their foreign implementation slots now delegate to
`PSRS.Array` or are implemented inline. `PSRS.Array` owns the target algorithms
over the private `arrayFill`, `arrayWrite`, `arrayIndex`, `arrayLength` and
integer primitives: range, replicate, `fromFoldable`, length, uncons, reverse,
concat, filter, partition, `scanl`/`scanr`, a stable merge sort, slice,
`zipWith`, `any`, `all` and `unsafeIndex`. The independent `psrs-stdlib`
revision is `332c69b16f240254f74375b7babed7f752f7361a` with content fingerprint
`fnv1a64-v1:3ab22be17dbadc5c`.

Two kinds of foreign slot could not be delegated directly. `Data.Array`'s
`index`, `findMap`, `findIndex`, `findLastIndex`, `insertAt`, `deleteAt` and
`updateAt` take rank-2 `Maybe` constructors or observers; their private foreign
signatures are written monomorphically and the loops are implemented inline over
the target helpers, leaving the public API unchanged. `Data.Array.NonEmpty.Internal`'s
`foldr1`/`foldl1` are inline `Fn2` wrappers and `traverse1` is implemented
directly in the `Traversable1` instance using the `Applicative` dictionary,
because its `apply`/`map` arguments are rank-2.

A compiler behavior is recorded rather than hidden: a recursive loop that writes
into its output array only in one branch of an `if` produced wrong results, while
the same loop written as a single tail call with the write in a `let` and only
the selected value chosen by the branch is correct. `PSRS.Array`'s loops use the
single-tail-call shape. The evaluation/alias behavior is a compiler obligation,
not a property of valid official source. `unsafeIndex` out of range traps
instead of returning JavaScript `undefined`, an explicit target difference.

Validation:

- Mandatory Wasmtime driver regressions passed, including the new
  `library_array_operations_match_pinned_official_observations` and the earlier
  apply, bind, extend, ST, uncurried and primitive checks.
- Official JS observations: the pinned `Data/Array.js` generates thirty cases
  with seventy-nine observable checks covering every delegated algorithm,
  boundary and empty inputs, negative and clamped slices, stable sorting and
  out-of-range access. The mandatory runner executes the Prelude-free target
  fixture under Wasmtime with exit 42 and empty output.
- Library-owned Node tooling regressions: 8 passed.
- Complete pinned audit: 41 packages, 217 modules, 189 exact, 19 modified and
  11 explicitly recorded target additions; three value foreign declarations are
  recorded as removed because their functionality moved into an inlined caller
  or instance. No absent official module and no detected exact direct
  same-argument recursion.
- Formatting and strict workspace clippy with warnings denied passed.

The full reproducer still fails. Unsupported-library P8 reports fell from 179 to
152 and the first is now `Data.Array.ST.unsafeFreezeImpl`; no `Data.Array` or
`Data.Array.NonEmpty.Internal` foreign value remains. Package content changed
from `fnv1a64-v1:03a4b91b0b29e0ec` to `fnv1a64-v1:3ab22be17dbadc5c`. Remaining
work in this chain is the mutable `Data.Array.ST` and `Data.Array.ST.Partial`
group. No full workspace tests or full scoreboard were run. No push, PR or issue
operation was performed.
