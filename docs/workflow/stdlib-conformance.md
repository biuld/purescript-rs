# Standard-library conformance commands

The standalone component is in `tools/stdlib-conformance`. It requires Python
3.9 or newer; scalar oracle generation also requires Node with ES module
support. Run it directly without installing dependencies:

```sh
export PYTHONPATH=tools/stdlib-conformance/src
python3 -m stdlib_conformance --help
```

A pinned complete source audit accepts the independent package and upstream
checkout directories. Supply the lock to reject a different upstream baseline:

```sh
python3 -m stdlib_conformance audit \
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
python3 -m stdlib_conformance scalar-oracle \
  --vendor ../psrs-stdlib/lib \
  --inventory ../psrs-stdlib/upstream-lock.json \
  --cases ../psrs-stdlib/conformance/scalars.json \
  --upstream purescript-prelude=/private/tmp/purescript-prelude \
  --upstream purescript-integers=/private/tmp/ps-pkgs/purescript-integers \
  --out /tmp/psrs-stdlib-oracle
python3 -m stdlib_conformance run \
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
