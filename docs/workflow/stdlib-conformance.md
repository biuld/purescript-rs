# Standard-library conformance commands

The library-owned Node component is `../psrs-stdlib/tools/conformance.mjs`.
It requires Node 22.7 or newer and has no npm dependencies. Run it directly:

```sh
node ../psrs-stdlib/tools/conformance.mjs --help
```

A pinned complete source audit accepts the independent package and upstream
checkout directories. Supply the lock to reject a different upstream baseline:

```sh
node ../psrs-stdlib/tools/conformance.mjs audit \
  --vendor ../psrs-stdlib/lib \
  --inventory ../psrs-stdlib/upstream-lock.json \
  --upstream /private/tmp/ps-pkgs \
  --upstream /private/tmp/purescript-prelude \
  --upstream /private/tmp/psrs-stdlib-audit-20261006/upstream \
  --out /tmp/psrs-stdlib-audit
```

The audit produces evidence for review; completing a report does not approve
its differences. Check the actual pinned checkout locations before running.
Generate and execute the currently implemented scalar cases:

```sh
node ../psrs-stdlib/tools/conformance.mjs scalar-oracle \
  --vendor ../psrs-stdlib/lib \
  --inventory ../psrs-stdlib/upstream-lock.json \
  --cases ../psrs-stdlib/conformance/scalars.json \
  --upstream purescript-prelude=/private/tmp/purescript-prelude \
  --upstream purescript-integers=/private/tmp/ps-pkgs/purescript-integers \
  --out /tmp/psrs-stdlib-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-stdlib-oracle/Golden.purs \
  --input /tmp/psrs-stdlib-oracle/Main.purs \
  --expected-exit 42 --out /tmp/psrs-stdlib-runtime
```

`run.json` separates compilation and execution. The default expected output is
empty stdout/stderr; use `--expected-stdout` and `--expected-stderr` files for
other byte-exact goldens. `--wasmtime` selects an executable, never an optional
skip. No runner command publishes or modifies upstream checkouts.

See [the repository boundary](../design/D-17-stdlib-and-conformance-boundaries.md)
for ownership, package locking, and the limits of this evidence.

For public Bounded values and Enum range interactions:

```sh
node ../psrs-stdlib/conformance/bounded.mjs \
  /private/tmp/purescript-prelude /tmp/psrs-bounded-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-bounded-oracle/Main.purs \
  --input /tmp/psrs-bounded-oracle/Target.purs \
  --expected-exit 42 --out /tmp/psrs-bounded-runtime
```

The generator verifies the six typed foreign-slot delegates against the pinned
source and reads its official JS bounds. Char's U+10FFFF target upper bound is
an explicit DEC-16 difference from JS's U+FFFF; the three Enum range observations
are integration checks. The library records these distinctions in
`docs/bounded.md` and `docs/evidence/bounded/`.

The library owns non-scalar case generators as well. For array application:

```sh
node ../psrs-stdlib/conformance/arrays.mjs \
  /private/tmp/purescript-prelude /tmp/psrs-array-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-array-oracle/Golden.purs \
  --input /tmp/psrs-array-oracle/Main.purs \
  --out /tmp/psrs-array-runtime
```

The generator verifies the clean upstream revision, evaluates the official JS
function, copies the actual vendored binding signature, and emits value checks.
Keep generators and cases in the library package; the independent runtime runner
continues to consume executable and source paths without compiler-internal APIs.

Tool changes and their tests belong in `psrs-stdlib`; run `npm test` there.
The compiler retains its package lock and Rust regression tests. A cargo xtask
wrapper, if added, should delegate to this CLI. The former Python component and
compiler-local compatibility scripts were removed after report-equivalent Node
validation. Historical evidence retains the commands used at that time.

For library array binding, generate the independent official observations and
run them through the same executable boundary:

```sh
node ../psrs-stdlib/conformance/array-bind.mjs \
  /private/tmp/purescript-prelude /tmp/psrs-bind-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-bind-oracle/Golden.purs \
  --input /tmp/psrs-bind-oracle/Main.purs --out /tmp/psrs-bind-runtime
```

Both array algorithms are PureScript target library implementations. The oracle
fixtures copy the official public signature and import that implementation;
there is no whole-function compiler intrinsic for either operation.

For array callback counts, visit order and short-circuiting:

```sh
node ../psrs-stdlib/conformance/array-callbacks.mjs \
  /private/tmp/ps-pkgs/purescript-arrays /tmp/psrs-array-callback-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-array-callback-oracle/Main.purs \
  --out /tmp/psrs-array-callback-runtime
```

These observations execute target helpers. Public `Data.Array` wrapper execution
remains blocked by unsupported `Data.Array.ST` bindings in its import closure;
helper acceptance does not establish public API or whole-library acceptance.

For scalar string length and slicing:

```sh
node ../psrs-stdlib/conformance/string-slicing.mjs \
  /private/tmp/ps-pkgs/purescript-strings /tmp/psrs-string-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-string-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-string-runtime
```

The generator verifies the five typed foreign-slot delegates and evaluates the
pinned official FFI on a one-BMP-unit-per-scalar projection. Scalar results map
back to original text; raw UTF-16 observations are recorded separately.
Negative-relative/clamping policies remain official, while DEC-16 intentionally
changes index units. This is a projected oracle, not raw-JS equality on
supplementary characters. Case engines and evidence live in the library package.

For scalar character indexing and single-character conversion:

```sh
node ../psrs-stdlib/conformance/string-characters.mjs \
  /private/tmp/ps-pkgs/purescript-strings /tmp/psrs-character-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-character-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-character-runtime
```

The shared String source verifier checks all seven typed foreign-slot delegates.
The character engine records 269 pinned-FFI projection observations separately
from 6 target-helper rank-N builder checks. Supplementary scalars intentionally
differ from raw JS code units under DEC-16; raw official results remain in the
observations. Public charAt and toChar wrappers execute unchanged.

For strict public integer parsing and Euclidean arithmetic:

```sh
node ../psrs-stdlib/conformance/int-parsing.mjs \
  /private/tmp/ps-pkgs/purescript-integers /tmp/psrs-int-parsing-oracle
node ../psrs-stdlib/conformance/int-arithmetic.mjs \
  /private/tmp/purescript-prelude /tmp/psrs-int-arithmetic-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-int-parsing-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-int-parsing-runtime
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-int-arithmetic-oracle/Main.purs \
  --expected-exit 42 --out /tmp/psrs-int-arithmetic-runtime
```

The parsing engine verifies the entire pinned Data.Int source, permitting only
its two typed foreign-slot adaptations. It records 836 actual official FFI
observations separately from 6 public Radix rejections and 4 target rank-N builder
checks. The arithmetic engine records 209 representable public degree/div/mod
observations. MIN / -1 produces an unrepresentable JS quotient and remains a
Wasm division trap; that observation is recorded separately from value agreement.
Both algorithms use raw truncating intQuot; the target library owns grammar,
overflow rejection and Euclidean sign correction.

For public radix integer formatting and its shared decimal Show implementation:

```sh
node ../psrs-stdlib/conformance/int-formatting.mjs \
  /private/tmp/ps-pkgs/purescript-integers /private/tmp/purescript-prelude \
  /tmp/psrs-int-formatting-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-int-formatting-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-int-formatting-runtime
```

The generator compares 857 public formatting results and 15 showInt results
against their actual pinned official JS functions, and records 857 parse/format
round trips and 5 named-base checks separately. All 2..36 bases and signed-i32
boundaries execute. The shared Data.Int source verifier now permits exactly
three typed foreign-slot adaptations; all official pure declarations remain.

For checked public Number-to-Int conversion:

```sh
node ../psrs-stdlib/conformance/int-number.mjs \
  /private/tmp/ps-pkgs/purescript-integers /tmp/psrs-int-number-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-int-number-oracle/Main.purs \
  --expected-exit 42 --out /tmp/psrs-int-number-runtime
```

The engine records 94 pinned official FFI observations, 11 toNumber/fromNumber
round trips and 6 target rank-N builder checks separately. It covers adjacent
IEEE-754 values at i32 boundaries, fractions, subnormals, signed zeros, overflow,
NaN and infinities. Target Int canonicalizes both Number zero signs to integer
zero; official negative-zero metadata is retained. The shared source verifier
now permits exactly four typed foreign-slot adaptations.

For finite Number classification and Number/Int truncation:

```sh
node ../psrs-stdlib/conformance/number-trunc.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /private/tmp/ps-pkgs/purescript-integers \
  /tmp/psrs-number-trunc-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-trunc-oracle/Main.purs \
  --expected-exit 42 --out /tmp/psrs-number-trunc-runtime
```

The engine verifies both pinned full-module transformations. It executes actual
Number isFinite/trunc FFI for 49 inputs and composes Int clamp expectations from
unchanged official source rules, producing 147 public API checks. Reciprocal
observations distinguish Number zero signs; Int uses its single integer zero.
Raw snapshots and runtime reports remain local and regenerable; commit only
source, case engines and concise acceptance reports.

For Number rounding and unchanged public Int clamping wrappers:

```sh
node ../psrs-stdlib/conformance/number-rounding.mjs \
  /private/tmp/ps-pkgs/purescript-numbers \
  /private/tmp/ps-pkgs/purescript-integers /tmp/psrs-rounding-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-rounding-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-rounding-runtime
```

The generator invokes the pinned official floor/ceil/round FFI, preserving
negative zero through reciprocal observations and testing adjacent values at
half-integer ties and Int bounds. Wasm floor/ceil are small checked primitives;
ECMAScript round tie behavior belongs to ordinary PureScript in PSRS.Number.
Official Data.Int pure wrappers remain unchanged. Raw generated reports are
local and ignored; summarized acceptance lives in the library's
`docs/number-rounding.md` and the compiler's topic report.

For Number absolute value through direct and higher-order public calls:

```sh
node ../psrs-stdlib/conformance/number-abs.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-abs-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-abs-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-abs-runtime
```

The generator checks the complete pinned Data.Number source transformation
and invokes its actual JS abs FFI. Reciprocal observations distinguish zero
signs. Cases include binary64 subnormals, nonfinite values, magnitudes beyond
i32, and deterministic generated bit patterns. The compiler's checked
numberAbs primitive selects f64.abs; the library retains its original public
Number signature and pure declarations.

For Number square root through the same public-call shape:

```sh
node ../psrs-stdlib/conformance/number-sqrt.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-sqrt-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-sqrt-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-sqrt-runtime
```

The generator invokes the pinned official sqrt FFI. Negative zero stays
negative zero, and negative inputs produce NaN. The compiler's checked
numberSqrt primitive selects f64.sqrt.

For Number inverse cosine through the same public-call shape:

```sh
node ../psrs-stdlib/conformance/number-acos.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-acos-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-acos-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-acos-runtime
```

The generator invokes the pinned official acos FFI. Finite inputs in [-1, 1]
match that FFI. Values outside the interval produce NaN. The compiler's
checked numberAcos primitive calls the scalar numeric-runtime export. Wasm
has no inverse-cosine instruction.

For Number inverse sine through the same public-call shape:

```sh
node ../psrs-stdlib/conformance/number-asin.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-asin-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-asin-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-asin-runtime
```

The generator invokes the pinned official asin FFI. Finite inputs in [-1, 1]
match that FFI, and negative zero stays negative zero. Values outside the
interval produce NaN. The compiler's checked numberAsin primitive calls the
scalar numeric-runtime export. Wasm has no inverse-sine instruction.

For Number inverse tangent through the same public-call shape:

```sh
node ../psrs-stdlib/conformance/number-atan.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-number-atan-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-number-atan-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-number-atan-runtime
```

The generator invokes the pinned official atan FFI. Every finite input and
both infinities match that FFI, and negative zero stays negative zero. NaN
produces NaN. The compiler's checked numberAtan primitive calls the scalar
numeric-runtime export. Wasm has no inverse-tangent instruction.
