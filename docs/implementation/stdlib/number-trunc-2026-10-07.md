# Number classification and truncation checkpoint

Starting compiler: c13a078, with a clean worktree. The library advances from
3e3736d to 0e3273498523ab81742ad0751efce5fde55e9794, fingerprint fnv1a64-v1:9ff76cd44757e7e6.

The compiler adds the generic unary numberTrunc :: Number -> Number primitive,
with authoritative HIR registry metadata and Core, CC and MIR type verification.
It lowers to f64.trunc, preserving Number range and signed zeros; NaN remains
NaN without a payload guarantee. New runtime evidence observes fractions,
values beyond i32 range, zero signs, NaN and infinities. Incorrect foreign
operand/result schemes and malformed CC/MIR operands are rejected.

Data.Number.trunc binds that primitive and isFinite delegates to the library's
IEEE-754 self-subtraction/equality check. The full official purescript-numbers
v9.0.1 source at 27d54effdd2c0e7a86fe356b1cd813dca5981c2d is verified,
allowing exactly these two foreign adaptations and their target import.
Data.Int and its public trunc/unsafeClamp wrappers remain unchanged. The
library owns finite classification and integer clamping, while the compiler
owns the scalar primitive and target encoding.

## Evidence

The pinned official FFI engine generates 147 public Number.isFinite,
Number.trunc and Int.trunc checks across 49 inputs. Number expectations execute
the actual official JS FFI; Int clamping expectations compose the unchanged
pinned PureScript rules. Programs built through development and locked roots both execute with exit
42 and empty stdout/stderr. The locked build removes PSRS_STDLIB_ROOT and
records compiler, source and Wasm digests locally. The original Int.trunc 42.9
probe changes from P8 missing isFinite and trunc bindings to Passed. Differing
library fingerprints preclude a compatible diagnose --compare claim.

Node tooling: 10 passed, zero skips. Source audit: 229 modules in 41 packages;
170 identical, 36 modified, 23 target additions, zero missing upstream modules
and zero direct recursive foreign placeholders. The development trusted-order
loader passes; Prelude remains first. Two focused mandatory-Wasmtime compiler
tests pass. Strict workspace clippy and formatting pass.

Combined validation: 1667 passed, 3 established baseline failures, 5 ignored.
The workspace commands cover 1666 passing tests; the additional post-audit
backend optimization test passes separately. Repeated target results are not
counted twice. Driver lib reports 689 passed and the same three pre-existing
artifact assertions: constrained_dictionary_parameters_precede_ordinary_arguments,
runs_a_polymorphic_identity_with_a_number, and
compiles_if_expression_through_cfg_to_structured_wasm. Failure names and messages
match the preceding deep-expression checkpoint. These remain failures; this is
not a green workspace result.

The default workspace command stops at driver lib, so the remaining targets
were explicitly executed:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
cargo test --workspace --exclude psrs-driver
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test '*'
cargo test -p psrs-driver --doc
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend optimized_and_unoptimized_number_trunc
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The backend optimization fixture executes both pipelines with exact exit 42,
observing negative zero through its reciprocal and preserving Number range
beyond i32. The scalar acceptance checklist records the SP-03/SP-07/SP-11/SP-12
mapping and evidence. All remaining crate, integration and doc tests pass;
the locked trusted-order loader also passes. Wasmtime version: 49.0.2.

Raw observations and reports remain untracked local artifacts. Reproduce with
[the conformance workflow](../../workflow/stdlib-conformance.md). This checkpoint
does not measure the full import cohort or an official-suite scoreboard and
does not establish all Number or standard-library APIs.

A fresh public `Int.floor 42.9` probe still reaches explicit P8 missing
Data.Number.floor support; other rounding operations remain a continuation.
