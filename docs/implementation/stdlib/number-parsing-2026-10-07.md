# Number parsing acceptance

## Contract and ownership

Starting compiler revision: a3ea5e8 on stdlib/vendor-core-libraries, with a
clean worktree. The starting library was 2ee2d1fdaff8a2841824cd1bb5d3fb864f594630
(fnv1a64-v1:801317111aa80a8f). The unchanged public Number.fromString "42.5"
wrapper reached P8 library linking, where Data.Number.fromStringImpl had no
target implementation. The preceding
[Core repair](explicit-polymorphic-arguments-2026-10-07.md) owns its explicit
polymorphic instantiation; this change adds no inference or Coercible rule.

The compiler owns numberFromDecimal :: String -> Number. It accepts a complete
ASCII decimal token, returns NaN for invalid tokens, and uses the pinned Rust
core binary64 converter for rounding, overflow and signed underflow. HIR/Core
check the String/Number contract; CC rejects incorrect operands and results
before erasure. MIR uses the existing canonical UTF-8 copy/release protocol to
call the raw runtime export with pointer and length. No pointer is retained.
Allocation failure remains a trap, including when the result is unused.

The independent library owns PSRS.Number.Parse: ECMAScript whitespace,
longest-prefix recognition, incomplete-exponent rollback, Infinity/NaN, and
the supplied predicate and polymorphic result callbacks. These are ordinary
target-library functions. The official pure public wrapper and foreign slot's
signature are preserved. Explicit type applications and intermediate functions
with explicit signatures construct Fn4 while retaining both universal arguments. The reduced
wrapper executes in psrs and compiles with official purs; a specialized value
cannot satisfy a universal argument.

The shared numeric runtime retains numberToString and adds numberFromDecimal.
Its reproducible artifact SHA-256 is
c6d50a6b005471bca9777562860cd8a3b2fc1ba5227126f755ee0d10297408de.
The linker checks the declared private table and active initializer exactly.
Static stack analysis follows exported/start entry points, rejects reachable
indirect calls, imported calls and recursion, and rejects exported tables.
Unreachable private formatting helpers do not establish execution paths.
The measured stack bound is 1680 bytes within the existing 65536-byte reserve.
Existing nonreentrant storage and allocator boundaries remain in force.

## Package and behavior evidence

Locked package: 4c4795a9c87496242398554b83f665062e3af2fd
(fnv1a64-v1:5e52e376007f1a23). Source adaptations, provenance, source verifier,
and Node oracle belong to psrs-stdlib; the compiler retains its lock, primitive,
linker contracts, runtime artifact, and Rust regressions.

```sh
node ../psrs-stdlib/conformance/number-parsing.mjs \
  /private/tmp/ps-pkgs/purescript-numbers /tmp/psrs-parsing-oracle
node ../psrs-stdlib/tools/conformance.mjs run \
  --compiler ./target/debug/psrs --stdlib-root ../psrs-stdlib \
  --input /tmp/psrs-parsing-oracle/Main.purs \
  --expected-exit 42 --timeout 240 --out /tmp/psrs-parsing-runtime
env -u PSRS_STDLIB_ROOT ./target/debug/psrs build \
  /tmp/psrs-parsing-oracle/Main.purs -o /tmp/psrs-parsing-locked.wasm
wasmtime /tmp/psrs-parsing-locked.wasm
```

The pinned official JS FFI supplies 304 checks: 244 public inputs and 60
callback, uncurried and partial-application checks. Coverage includes every
ECMAScript whitespace code point, rejected lookalikes, incomplete exponents,
trailing text, nonfinite results, negative zero, subnormals, exact rounding
ties, long tokens, and 128 deterministic generated decimal cases. Custom
predicates and builders exercise accepted/rejected NaN and Infinity paths.
Both explicit-package and locked-package executions return 42 with empty
stdout/stderr. All 414 public Number/Int rounding checks also pass against the
final package. A fresh
locked-package diagnosis of the original public reproducer passes. Since the
package fingerprint changed, this is replay across a package update rather
than a same-fingerprint diagnose --compare measurement.

The full source inventory covers 230 modules across 41 pinned packages:
170 identical, 36 modified, 24 target additions, no absent upstream modules,
and no direct recursive foreign placeholders. This inventory does not approve
all pre-existing adaptations or establish whole-library runtime behavior.
The Node tooling tests pass: 11 passed, zero skipped. The numeric runtime
rebuild is byte-for-byte reproducible. Raw reports, logs, oracle programs, and
execution artifacts remain under /private/tmp and are not committed.

## Rust validation

The four focused driver regressions pass with mandatory Wasmtime: official
public wrapper execution, decimal precision/range/zero signs, invalid foreign
schemes, and explicit universal newtype arguments. Native runtime tests cover
decimal grammar and rounding boundaries. CC malformed-operation checks and
linker tests cover element-contract mismatches and reachable stack paths.

`cargo fmt --all --check`, `git diff --check`, and
`CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings`
pass. All 230 vendored source hashes match the final package's inventory.

`CARGO_INCREMENTAL=0 PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`
completes all 52 target suites, including doc tests: 1683 passed, three existing
failures, and five ignored. The failures match the preceding Core checkpoint:

- `dictionary_audit::execution::constrained_dictionary_parameters_precede_ordinary_arguments`:
  the test cannot find its expected MIR constrained function.
- `tests::functions::runs_a_polymorphic_identity_with_a_number`:
  its optimized WAT does not contain the asserted f64 token.
- `tests::integration::compiles_if_expression_through_cfg_to_structured_wasm`:
  its optimized constant branch does not contain the asserted br_if token.

The first full run additionally exposed a stale formatter-artifact identity
assertion. It now checks the numeric artifact identity and its pinned digest;
both its focused rerun and the complete workspace rerun pass that regression.
Full workspace validation is therefore not green because of the three baseline
failures, with no new failing test in the completed rerun.

## Remaining work

This establishes the parsing slice. It does not establish complete stdlib
compilation, source fidelity of every adaptation, or behavior of every API.
A fresh public Number.abs (-42.5) case stops at P8 library linking because
Data.Number.abs has no target implementation. Other Number foreign slots
remain separate work. No official scoreboard or gate measurement changes.

The subsequent [absolute-value acceptance](number-abs-2026-10-07.md) resolves
the abs blocker with a checked scalar operation and public JS comparisons.
The measurements above describe the preceding parsing checkpoint.
