# Vendored standard-library source audit

## Result and policy

The current vendored library does **not** satisfy the policy that differences
from official PureScript libraries must be justified by the Wasm/WASI target.
There are 200 exact self-recursive placeholder definitions in 30 modules,
removed official APIs and instances, and changes to ordinary PureScript
functions made to accommodate compiler limitations. These are defects, not
accepted target adaptations.

This audit inspects revision `67369ba` on `stdlib/vendor-core-libraries` with a
clean starting worktree. Library sources were not changed. The audit adds only
this report, its evidence, and a repeatable inventory tool. No scoreboard was
run or updated.

The successful `/tmp/psrs-stdlib-all.purs` diagnosis establishes acceptance of
these modified sources, with `main = 0`. It does not establish acceptance of
unchanged official sources or execution correctness of the exported APIs.
Unused declarations can disappear before backend layout and execution.

## Baselines and coverage

The original vendoring script `/tmp/vendor_stdlib.py` names `/tmp/ps-pkgs` and
`/tmp/purescript-prelude` as inputs. Its `render_foreign` operation deliberately
replaces an unsupported value foreign import with a self-recursive equation.
Its `EXACT` list also rewrites `void`, `voidRight`, `voidLeft`, and `fold`.
Commit `0118695ffffd71200ece889cf863086955c03a64` explicitly records that value
foreign imports become intrinsics or diverging stubs. The script was inspected
without running its source-writing entry point.

All 41 upstream checkouts are clean, at exact version tags, and have
`https://github.com/purescript/...` origin URLs. Their tags, commits, paths,
and source SHA-256 hashes are captured in [inventory.json](inventory.json).
The compiler support manifest at
`/Users/biu/Projects/purescript/tests/support/bower.json` uses dependency ranges,
not a lockfile. These tags identify the available vendoring baselines; they do
not prove a uniquely pinned dependency set for compiler version 0.15.16.
The original 38 checkouts are supplemented by official effect v4.0.0,
console v6.0.0, and assert v6.0.0, downloaded after explicit user approval.
These versions satisfy the compiler support manifest's dependency ranges;
they are explicit comparison baselines, not a recovered original lockfile.
Four upstream modules from the supplemental packages are absent from the
vendored directory and are listed below.

| Comparison of current `.purs` modules | Count |
| --- | ---: |
| Byte-identical to the available official baseline | 149 |
| Different only in final line termination | 3 |
| Source changes requiring review | 50 |
| Wasm/WASI platform additions without an official counterpart | 9 |
| Official baseline unavailable | 0 |
| Total current vendored modules | 211 |

The 202 compared modules come from prelude v6.0.1 and 40 other tagged packages.
The complete per-module table is [modules.md](modules.md); the unfiltered patch
is [official-vs-vendored.diff](official-vs-vendored.diff). Newline-only changes
are in Data.HeytingAlgebra.Generic, Data.Ring.Generic, and Data.Semiring.Generic.

The initially missing baselines are now verified against both clean tagged git
checkouts and independently downloaded tagged archives. The official commits
are effect `a192ddb923027d426d6ea3d8deb030c9aa7c7dda`, console
`3b83d7b792d03872afeea5e62b4f686ab0f09842`, and assert
`27c0edb57d2ee497eb5fab664f5601c35b613eda`. All current non-WASI modules have an
official comparison baseline; no baseline remains unavailable.

## Confirmed violations

### Foreign declarations replaced by nontermination

Across the compared present modules, 267 upstream value foreign declarations
have changed: 200 have exact self-recursive replacements, 40 have nonrecursive
replacement declarations, and 27 declarations have no local replacement
signature. The last category includes values now re-exported from another
module or supplied as compiler primitives; it does not mean that all 27 public
values are absent. A
nonrecursive replacement is only a syntactic classification, not a correctness
judgment. All 200 detected placeholders correspond to original foreign values.
Their names and vendored line numbers are in the inventory. The count excludes
other possible incorrect implementations or forms of nontermination.

For example, Data.Int replaces the official `foreign import toNumber` with
`toNumber a0 = toNumber a0`. Data.Array replaces a foreign `rangeImpl` with
`rangeImpl = rangeImpl`. Similar replacements affect ST operations, reference
mutation, uncurried functions, array construction and traversal, numeric
functions, regular expressions, string operations, partiality, and reflection.
A declaration being unreachable or supplied through a compiler interface does
not make its on-disk source replacement faithful.

| Module containing placeholders | Definitions |
| --- | ---: |
| Control.Extend | 1 |
| Control.Monad.ST.Internal | 11 |
| Control.Monad.ST.Uncurried | 20 |
| Data.Array.NonEmpty.Internal | 3 |
| Data.Array.ST.Partial | 2 |
| Data.Array.ST | 17 |
| Data.Array | 24 |
| Data.Enum | 2 |
| Data.Foldable | 2 |
| Data.Function.Uncurried | 20 |
| Data.FunctorWithIndex | 1 |
| Data.Int | 7 |
| Data.Lazy | 2 |
| Data.Number.Format | 4 |
| Data.Number | 25 |
| Data.Reflectable | 1 |
| Data.String.CodePoints | 7 |
| Data.String.CodeUnits | 15 |
| Data.String.Common | 8 |
| Data.String.Regex | 10 |
| Data.String.Unsafe | 2 |
| Data.Symbol | 1 |
| Data.Traversable | 1 |
| Data.Unfoldable | 1 |
| Data.Unfoldable1 | 1 |
| Effect.Ref | 5 |
| Partial.Unsafe | 1 |
| Partial | 1 |
| Record.Unsafe | 4 |
| Unsafe.Coerce | 1 |

The loader excludes compiler-provided Safe.Coerce and Unsafe.Coerce source
modules, so the Unsafe.Coerce placeholder is not evidence that every ordinary
use of the compiler primitive diverges. Data.Symbol retains source ownership;
its local `unsafeCoerce` placeholder is a separate declaration from the
canonical Unsafe.Coerce compiler interface. Static IsSymbol dictionary
synthesis does not establish runtime reification through this local body.

### Removed pure APIs and instances

**Data.Tuple:** upstream v7.0.0 declares 24 instances; the vendored module keeps
four. It removes Eq1, Ord1, Bounded, Semigroupoid, Semigroup, Monoid, Semiring,
Ring, CommutativeRing, HeytingAlgebra, BooleanAlgebra, Generic, Invariant, Apply,
Applicative, Bind, Monad, Extend, Comonad, and Lazy instances. The ADT is already
`data Tuple a b = Tuple a b` in both sources. Distinguishing that ADT from a WIT
record tuple provides no justification for removing its ordinary instances.

**Data.Show:** the vendored module removes the exported ShowRecordFields class
and showRecordFields method, the Show instances for Proxy, Void, and records,
and all three ShowRecordFields instances. These are pure API removals, not
UTF-8 storage adaptations. Its header explicitly acknowledges that Number
rendering can differ from official correctly rounded formatting. A Wasm
implementation of numeric/string formatting needs behavior evidence; target
storage does not justify silently narrowing the class surface or approximating
unrelated numeric behavior.

**Test.Assert:** the exact v6.0.0 diff confirms removal of six public functions:
assertEqual, assertEqual', assertTrue', assertFalse', assertThrows, and
assertThrows'. The first four are ordinary PureScript functions with no target
implementation requirement of their own. The current source documents omission
of the equality functions because of a compiler constraint-elaboration defect;
that rationale violates the policy. Replacing JavaScript assertion exceptions
with a Wasm trap, and limitations on observing a Wasm trap inside the guest,
are target-related concerns requiring a declared contract. They do not justify
removing the comparison and message-taking Boolean assertions.

**Effect:** the exact v4.0.0 diff confirms removal of whileE, forE, and foreachE,
plus the pure semigroupEffect and monoidEffect instances. The ordinary Effect
Functor/Apply/Applicative/Bind/Monad instances and type ownership were moved to
Prelude, and the source role declaration was removed. The target state-token
protocol can justify implementation and ownership adaptations, but not deletion
of the two pure instances or omission of the standard loop APIs. The vendor
also exports pure, bind, discard, map, and apply, which upstream Effect does not
export. These additions belong in the reviewed target-interface inventory.

**Effect.Console:** the exact v6.0.0 diff confirms removal of ten public values:
warnShow, errorShow, info, infoShow, debug, debugShow, time, timeLog, timeEnd, and
clear. The Show wrappers are pure composition over existing console operations;
warnShow and errorShow need no new host capability. Direct writes routed through
WASI.Console are target adaptations. JavaScript console timers/clear and
severity routing need explicit target decisions; they do not justify silently
removing the pure wrappers or presenting this reduced module as the full API.

### Whole official modules omitted

The supplemental packages also contain these four modules, all absent from the
current vendor. The inventory records their upstream hashes and links, and the
full diff records them as removals to `/dev/null`.

| Official module | Source contract and disposition |
| --- | --- |
| Effect.Class | Pure MonadEffect class, liftEffect method, and Effect instance. No Wasm-specific reason for omission. |
| Effect.Class.Console | Pure MonadEffect lifting wrappers for all console operations. No Wasm-specific reason for wholesale omission. |
| Effect.Uncurried | EffectFn types and mk/run conversions. Requires a target calling-convention implementation or explicit unsupported bindings. |
| Effect.Unsafe | unsafePerformEffect foreign binding. Requires the checked target Effect execution protocol or explicit unsupported status. |

### Ordinary source functions changed for the compiler

The original `void = map (const unit)`, `voidRight x = map (const x)`,
`voidLeft f x = const x <$> f`, and `fold = foldMap identity` have been rewritten.
These are valid ordinary PureScript definitions and have no Wasm/WASI-specific
semantics. Restore the official forms and repair compiler handling at its
owning stage if necessary.

Instance methods were also eta-expanded in Data.Eq, Data.EuclideanRing,
Data.Functor, Data.HeytingAlgebra, Data.Ord, Data.Ring, Data.Semigroup, and
Data.Semiring. The vendoring script explains this through its restriction that
bare intrinsics are not first-class values. A target binding can have a wrapper,
but that compiler limitation does not by itself authorize rewriting the
upstream instance definitions. Preserve the foreign value's source contract
and put implementation adaptation at the defined primitive boundary.

### An incorrect FFI replacement

Official `Data.Eq.js` checks the two array lengths before reading any element.
The vendored `eqArrayFrom` stops only when its index reaches the left array's
length. It can read beyond the right array's bounds when the left array is
longer. For `[1] == []`, the official operation returns false without reading
an element; the replacement attempts to index the empty right array. This is
an observable algorithm difference found by source inspection; this audit did
not execute a Wasm reproduction or claim a measured trap.

## Target-related changes requiring separate evidence

The other changed files must not be automatically approved merely because
an upstream foreign declaration was replaced. The following table accounts for
all 20 modified modules without an exact self-recursive placeholder. The other
30 modified modules are covered by the placeholder inventory above.

| Module | Difference and disposition |
| --- | --- |
| Control.Apply | Replaces arrayApply FFI with recursive PureScript helpers. Target implementation candidate; equivalence and stack behavior unverified. |
| Control.Bind | Replaces arrayBind FFI with recursive PureScript helpers. Target implementation candidate; equivalence and stack behavior unverified. |
| Data.Bounded | Replaces numeric/character constants. Primitive representation candidate; validate bounds against the target Char/Number contracts. |
| Data.Eq | FFI replacements plus method rewrites; confirmed array bounds difference and unapproved pure rewrites. |
| Data.EuclideanRing | Removes intDiv/intMod foreign declarations and uses intrinsics, implements intDegree/numDiv, rewrites methods. Verify negative and zero divisors; pure method rewrites lack target necessity. |
| Data.Functor | Implements arrayMap with recursion, eta-expands map, rewrites three pure combinators. Pure combinator changes violate policy. |
| Data.HeytingAlgebra | Boolean primitive wrappers plus instance-method rewrites. Primitive implementation candidate; pure rewrites require restoration. |
| Data.Int.Bits | Seven FFI values call Wasm-oriented integer intrinsics. Target implementation candidate; validate bit and shift semantics. |
| Data.Ord | Numeric, character, array, and UTF-8 byte comparators plus method rewrites. UTF-8 ordering must follow DEC-16; method rewrites are compiler accommodations. |
| Data.Ring | Removes intSub declaration and implements numSub; rewrites both methods. Primitive bindings need an explicit target boundary, not source-method workarounds. |
| Data.Semigroup | Implements string concatenation through UTF-8 bytes and array concatenation through intrinsics, rewrites append methods. Storage adaptations are candidates; method rewrites are not established as necessary. |
| Data.Semiring | Removes intAdd/intMul declarations, adds numeric primitive wrappers, rewrites methods. Primitive adaptation candidate; method rewrites are compiler accommodations. |
| Data.Show.Generic | Replaces intercalate FFI with a PureScript helper. Target implementation candidate; output/order/boundary behavior unverified. |
| Data.Show | Target rendering implementation mixed with API removals and acknowledged Number behavior differences. Fails policy as a whole. |
| Data.Tuple | Pure implementation replaced and 20 instances removed. Fails policy. |
| Data.Unit | Replaces foreign type/value declarations with re-exports of compiler builtins. Requires an explicit primitive identity/representation justification; not blanket-approved. |
| Prelude | Adds Effect, runEffect, trap and their primitive bindings/instances. Documented target interface, but relocation and extra exports require API/dependency review rather than an assumption of upstream equality. |
| Effect | Target type and instances relocated to Prelude, standard loops and two pure instances omitted. Fails policy as a whole. |
| Effect.Console | WASI routing mixed with ten omitted values, including pure Show wrappers. Fails policy as a whole. |
| Test.Assert | Trap-based failure mixed with removed equality and message-taking Boolean assertions. Fails policy as a whole. |

The nine WASI modules are platform additions: WASI, WASI.Resource, WASI.IO,
WASI.Clock, WASI.Random, WASI.Console, WASI.Process, WASI.FileSystem, and
WASI.Network. Their scope is target-related. This audit does not assert their
runtime correctness. Effect, Effect.Console, and Test.Assert are handwritten
platform wrappers whose exact differences are now included in the audit.

DEC-16 permits target-specific Unicode scalar and UTF-8 behavior; it does not
permit dropping pure instances or using nontermination to fake an FFI value.
A reviewed adaptation must identify the target requirement and preserve the
public source contract wherever that requirement does not explicitly change it.
Unsupported bindings must remain explicit compile/link failures rather than
apparently implemented recursive functions.

## Reproduction and next obligations

Run from the repository root with the same clean upstream checkouts:

```sh
python3 docs/workflow/tools/audit-stdlib-vendor.py \
  --vendor stdlib/lib \
  --upstream /tmp/ps-pkgs \
  --upstream /tmp/purescript-prelude \
  --upstream /tmp/psrs-stdlib-audit-20261006/upstream \
  --out /tmp/psrs-stdlib-audit-repeat
```

The tool checks clean tagged upstream checkouts, rejects duplicate module paths,
records byte hashes, emits an unfiltered unified diff, reports exact top-level
self-recursions, and flags changes to original code outside value FFI declaration
spans. Those flags are review evidence, not semantic approval. Matching source
paths and package tags do not certify behavior of generated primitive bindings.

Required continuation:

1. Keep all 41 comparison baselines pinned and preserve their source provenance.
2. Restore removed pure APIs/instances, omitted pure modules, and the ordinary
   upstream function forms.
3. Restore unsupported foreign declarations instead of recursive placeholders;
   connect supported values through the explicit Wasm/WASI primitive contract.
4. Keep target adaptations reviewable with their rationale and behavior tests,
   including UTF-8 differences under DEC-16.
5. Rerun source compile acceptance after restoration; expect honest compiler or
   linking blockers until the required target implementations exist.
6. Run focused value-sensitive runtime comparisons for each implemented binding.
   A complete runtime scoreboard remains a separate measurement.

No library repair, compiler change, push, PR, or issue operation was performed
as part of this source audit.
