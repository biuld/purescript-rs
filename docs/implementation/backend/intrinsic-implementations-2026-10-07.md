# Intrinsic implementation separation acceptance — 2026-10-07

## Contract and baseline

The governing contracts are [intrinsic implementations](../../design/backend/fp/intrinsic-implementations.md),
[D-15](../../design/D-15-compiler-builtins.md),
[scalars](../../design/backend/fp/scalars-and-primitives.md), and
[DEC-18](../../decision/DEC-18-unified-target-linking.md).
The compiler baseline was clean `b263b14` on `stdlib/vendor-core-libraries`;
the library baseline was `e0679521275277c0b005b2509d9e0a8570715c5e`.

A focused source diagnosis accepted the old explicit `intDiv/intMod` bindings.
Execution demonstrated that they returned floor-policy results different from
public official Euclidean arithmetic for negative divisors. This was a duplicate
compiler policy, not a reason to rewrite official library functions.

A separate malformed-MIR reproducer declared a formatter consumer with
`(i32, i32, i32) -> i32`, and made its call agree with that declaration.
MIR verification and Wasm lowering both accepted it before this change. After
this change local MIR verification still accepts the internally consistent
call, but target planning rejects it against the formatter's actual
`(f64, i32, i32) -> i32` provider. Both direct Wasm lowering and the checked-plan
path enforce this boundary. Baseline reports, raw programs and logs remain
outside the repository under `/private/tmp/psrs-intrinsic-*`.

## Implemented ownership

- HIR owns stable language identities, schemes, arity and conservative trap/mutation effects.
- The exhaustive backend catalog selects direct, generated or artifact implementations, with explicit elaborated/unsupported cases. Artifact enumeration derives from this selection.
- CC verifies artifact arguments and results before erasure. MIR adapts calls through a shared raw-scalar/UTF-8 protocol instead of per-numeric-operation assignments.
- Runtime metadata owns raw export ABI, output capacity and borrowed/caller-owned buffer protocols. Existing runtime executable bytes are retained.
- Linking derives expected ABI from actual MIR imports. Generated allocator/codec signatures are shared by MIR import production, provider verification and emission.

Language `Int*` names replace HIR machine spellings while preserving active
symbol IDs. Retired floor IDs 26 and 27 cannot be reused or resolved as bindings.
The old CC operations, MIR helpers and helper-only tests are removed. Public
Euclidean division/modulo remain in ordinary library source, including zero
handling. Two target-only array helpers use `intQuot` for nonnegative lengths
and positive divisors; their official source callers remain unchanged.

## Verification

Measured on 2026-10-07 against compiler `b263b14` plus this change, with the
lock at library `7cace2a6b4808b36b01559e88d3ebc351a3b7426`
(`fnv1a64-v1:1fd8c04ec63c6ff0`). Wasmtime is 49.0.2. Cargo commands used
`CARGO_INCREMENTAL=0`. Runtime checks also used `PSRS_REQUIRE_WASMTIME=1`.
Raw logs stay outside the repository under `/private/tmp/psrs-intrinsic-*`.

Compiler checks:

- `cargo fmt --all --check` passed on the final Rust sources.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test -p psrs-backend --lib`: 408 passed.
- `cargo test -p psrs-driver --lib tests::scalars`: 16 passed. This includes
  the repaired case-arm division test. It no longer requires a compiler floor
  helper.
- `cargo test -p psrs-driver --lib intrinsic`: 7 passed. Retired `intDiv` and
  `intMod` bindings are rejected. Public Euclidean division and modulo run for
  both signs, zero divisors, and a higher-order call. A balanced public
  `Maybe` traversal covers nine elements, an empty input, and a failed element.
- `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast` finished all
  52 targets: 1690 passed, 3 failed, 5 ignored. The only failing target is
  `psrs-driver --lib`, and its three failures are the established baseline:
  `constrained_dictionary_parameters_precede_ordinary_arguments` (the expected
  constrained MIR function is absent),
  `runs_a_polymorphic_identity_with_a_number` (optimized WAT has no `f64`),
  and `compiles_if_expression_through_cfg_to_structured_wasm` (optimized WAT
  has no `br_if`). No new failure appeared.
- `crates/psrs-runtime/tools/check-reproducible.sh` rebuilt the runtime
  artifact and found it byte-for-byte identical to the committed Wasm.
- A direct malformed formatter consumer, `(i32, i32, i32) -> i32`, still
  passes local MIR verification. Target planning and Wasm lowering both reject
  it against the formatter export `(f64, i32, i32) -> i32`.

These checks do not by themselves establish library behavior. Source fidelity,
compilation, and runtime agreement are separate.

Library evidence, with the package fingerprint unchanged during each run:

- Source audit of the locked package: 170 identical, 36 modified, 24 platform
  additions, 230 modules, and no upstream module absent from the vendor. The
  audit records differences. It does not approve them.
- `npm test` in `psrs-stdlib`: 11 passed.
- The library-owned Node runner accepted these oracles. Integer arithmetic has
  209 official observations; the unrepresentable JavaScript quotient of the
  signed minimum divided by `-1` is excluded, and the raw Wasm primitive traps.
  Public Show has 333 observations, with stdout exactly `3\n1\n2\n`, empty
  stderr, and exit 42. `Number.abs` has 306 checks, decimal parsing 304
  observations, and Number/Int rounding 414 checks. Array application has 8
  cases and 29 equality checks. Array algorithms, including sorting, have 30
  cases and 79 equality checks. Every non-Show command exits 42 with empty
  stdout and stderr.
- The integer oracle built from the locked package, without a development
  package override, is sha256
  `020e5fbf004c167f74f3a149a8f40d84bb0f9a68424187db5f9f42d960194674`. Wasmtime
  executed that artifact with exit 42 and empty stdout and stderr.

This refactor does not establish whole-stdlib runtime completion. `Data.Number.sqrt`
remains blocked by unsupported target FFI.
