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
