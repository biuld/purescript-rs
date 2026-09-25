# Effects Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Effects](../../design/backend/fp/effects.md)

**Progress:** Unverified; audit existing implementation and tests first.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21, with BE-02 and BE-26 at closure/library boundaries.

## Scope and dependencies

Complete the linked design's `Effect a` representation, `pure`, `bind`,
`runEffect`, hidden execution token, and sequencing through CC/MIR/Wasm. A
computation value is inert until an authorized runner invokes it. The linked
design's present-tense contract is authoritative beyond this matrix. Source
do/ado desugaring and class elaboration are frontend inputs; WASI service
availability belongs to the platform topic. This topic must verify the
ordinary closure interface and the trusted entry boundary. Reconcile any
current embedded-library token representation with the design's internal-token
contract before marking that row Verified.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row needs behavior-sensitive execution, not only a closure-shaped IR.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| EF-01 | `Effect a` is abstract to source code and represented internally as a closure from hidden token to result, with no effect-specific CC/MIR node. | Inspect type/library boundary and generated CC/MIR; negative source fixture cannot forge a runner or token. | Unverified |
| EF-02 | Constructing, storing, passing, returning, or capturing an Effect value performs no action. | Compile programs that build and discard or store an effect; mandatory execution asserts no import call/output before run. | Unverified |
| EF-03 | `pure` returns the supplied value when run and invokes no external action. | Source and verified Core cases for scalar/reference values; inspect closure call count and value-sensitive result. | Unverified |
| EF-04 | `bind` runs the first effect before applying the continuation and then runs the returned effect exactly once. | Observable output/call-count order, including a continuation that ignores its argument, nested binds, and expected traps. | Unverified |
| EF-05 | `runEffect` is available only to trusted entry/runtime code and invokes the closure once per call. | Unauthorized source call fails with source-associated diagnostic; two authorized runs produce two actions and a single run one action. | Unverified |
| EF-06 | Partial application captures supplied arguments once and defers action until invocation. | Side-effecting argument/continuation cases inspect CC captures and execute repeated runs without repeated construction-time evaluation. | Unverified |
| EF-07 | Core/P8 preserve strict source order of `let`, effect construction, and effect execution. | Source order cases with distinguishable WASI outputs, failures, and nested/conditional effects; compare Core, CC and runtime sequence. | Unverified |
| EF-08 | Polymorphic `Effect a` uses the normal erasure, boxing and closure adapters without exposing the token. | Execute effects returning Int, Number, String and a GC aggregate through generic functions; inspect signatures and recovered values. | Unverified |
| EF-09 | Linked source modules forward Effect values without running them or granting untrusted modules runner privilege. | Producer/consumer modules with delayed execution, repeated forwarding, and unauthorized `runEffect` attempt. | Unverified |
| EF-10 | Wasm/component entry executes only the selected trusted action and preserves WASI call order, results, and failures. | Mandatory Wasmtime component execution with stdout/stderr or another observable import, call counts, exit behavior, and valid binary. | Unverified |
| EF-11 | Malformed internal closure/token signatures fail verification before encoding. | CC/MIR negative fixtures for wrong token position, return shape, call arity, and unauthorized imported runner binding. | Unverified |

## Vertical execution order

1. Audit library imports, trusted entry, Core-to-CC lowering, closure layout,
   component runner, diagnostics, and tests against each design section.
2. Resolve token and privilege mismatches, then implement `pure`/`bind`/run
   semantics and verifier rules with exact negative tests.
3. Execute inertness, order, repeated-run, polymorphic, and cross-module cases
   through the normal component path with mandatory Wasmtime.
4. Record evidence and update D-04. Do/ado syntax and unrelated WASI services
   remain separately owned; do not claim them from Core fixtures.

## Evidence record and completion rule

For each ID record code paths/functions, exact tests/assertions, input
boundary, commands, Wasmtime version, actual executions/skips, revision, and
gaps. Distinguish source privacy from a trusted test fixture. Runtime cases
require `PSRS_REQUIRE_WASMTIME=1`; a skip is not verification. After Rust edits
run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused runtime
cases. Close only when all rows and the complete present-tense design pass.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
EF-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. In particular, verify the hidden-token and trusted
runner boundaries before accepting existing source-order tests.
