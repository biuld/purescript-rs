# Independent stdlib package checkpoint

The library subtree was extracted into the local `psrs-stdlib` repository with
its Git history. All 216 library files (215 `.purs` modules and `lib/trusted`)
matched the compiler copy byte for byte before that copy was removed. Package
metadata, upstream pins, conformance cases, and repository guidance were then
committed as `a08b4c8`. The compiler's `stdlib.lock.json` selects that package
revision and content fingerprint. No remote repository, push, or PR was created.

The standalone component in `tools/stdlib-conformance` accepts package and
executable paths. Source audit and scalar cases ran against the external package.
The audit covered all 41 pinned packages: 195 identical modules, 11 modified
modules, nine platform additions, no omitted upstream modules, and no detected
direct same-argument self-recursions. These counts describe differences; they do
not approve every adaptation or prove full FFI implementation.

Validation:

- Package identity/protocol/dirty-content tests: three passed.
- Trusted on-disk loader: one passed after removing the compiler source copy.
- Primitive foreign tests with mandatory Wasmtime: seven passed.
- Let-constraint regressions: 14 passed.
- Official `purs` Symbol differential: one passed, using the unchanged official
  `.purs` and its pinned upstream JavaScript companion (fixture trailing blank line
  omitted).
- Independent official scalar oracle: 20 bindings, 46 cases; actual Wasmtime
  exit 42, empty stdout and stderr. `scalar-run.json` records binaries, input
  hashes, commands, package identity, Wasm hash, and observations. Signed i32
  normalization remains an explicit representation difference in the oracle.
- Invalid package overrides and incorrect exit-code goldens failed explicitly.
- CLI build, formatting, and workspace clippy with warnings denied passed.
- Python syntax compilation and Node syntax checking passed. The component
  also ran successfully after copying it outside the compiler checkout and
  against a Git-free archive of the locked library package.

Full compile acceptance remains failed: `/tmp/psrs-stdlib-all.purs` reported
233 P8 library-linking diagnostics; the first was the missing target
implementation of `Control.Apply.arrayApply`. `summary.json` records this
measurement. No full workspace tests or full scoreboard were run. No roadmap
runtime figures were changed.

Independent source acquisition for fresh checkouts/CI and remote publication
remain pending. The package's development status is incomplete. Full stdlib
compilation and value-sensitive runtime/FFI correctness remain separate open
obligations; the scalar subset is not a whole-library acceptance claim.
