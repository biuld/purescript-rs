# Deep expression traversal checkpoint

Starting point: compiler 502930d on stdlib/vendor-core-libraries, with an
uncommitted compiler repair handed over for review. The independent library
remains pinned to c8020c00e227f256c0a365e22d5ebcbd1601607a, fingerprint
fnv1a64-v1:6d4f0e0ccbe37bc0. No library sources are changed by this topic.

## Boundary and repair

The ordinary Prelude expression `true && ... && true` with 448 operands was
accepted by official purs but aborted this compiler. The ungrouped String
oracle exposed recursive P5 inference frames; on a 2 MiB worker stack, a later
backtrace located Expr::clone within Core lower_expr_inner. Saturated intrinsic
lowering cloned the already lowered right remainder while ancestor frames
were still live.

The owning contracts are P4 fixity and P6 checked intrinsic lowering in
[D-01](../../../design/D-01-frontend-and-ir-boundaries.md). Fixity and evaluation
order are preserved: operator association still determines the application
tree. Intrinsic recognition still requires a registered global head and exact
descriptor arity. The saturated path consumes the application spine and
reverses its collected arguments into source order, avoiding recursive copies.
Partial and oversaturated calls continue through ordinary application lowering.

Expression entry points in lowering, checking, verification, optimization and
capture analysis use psrs_span::with_sufficient_stack. It checks a 256 KiB red
zone and continues on an 8 MiB heap stack segment. Recursive entries must pass
through the wrapper; checking only once at the pass root is insufficient.
HIR, THIR and Core Expr clone and equality use the same mechanism at each node.
The change introduces no Boolean/String name exception, re-association or new
runtime primitive. It does not claim arbitrary-depth safety for every tree
operation: ordinary derived destruction and Debug formatting are unchanged;
destruction is exercised at depth 800.

## Validation

`PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast` completes with
1664 passed, 3 failed and 5 ignored. The three failures are the same previously
reproduced [baseline assertions](../compile-progress-2026-10-07/validation.json):
dictionary parameter ordering expects a pruned function name, polymorphic
Number identity expects an unused f64 value in WAT, and a constant conditional
expects a br_if after optimization. Driver-library results are 687 passed and
3 failed. All other workspace targets, including documentation tests, pass.
The full log is /private/tmp/psrs-stack-workspace-20261007.log.

`cargo fmt --all --check`, `git diff --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass. validation.json
records the commands, aggregate outcomes and the unchanged baseline failures.

Runtime evidence for the original ungrouped String
fixture is in string-run.json; all 448 projected FFI value checks execute under
Wasmtime 49.0.2 and return 42 with empty stdout/stderr. This extends the grouped
String checkpoint without changing its scalar-indexing oracle or source
fidelity claim.

The additional right-associative subtraction test uses 448 operands: its
expected result is 0; left association would produce -446. Official purs
accepts the same source; subtraction-run.json records Wasmtime execution.
Both new driver tests pass within the full required-runtime workspace run.
HIR, THIR and Core tests exercise cloning, equality
and destruction of an 800-node application spine, and psrs-span directly
exercises stack growth with large recursive frames.

The 211-module import fixture passes in 114081 ms with a 240-second timeout,
zero exclusions, crashes or timeouts. import-summary.json records the compiler,
input and library fingerprints and captured pass/validation records. An unused integer
entry point establishes import compilation only, not execution of every
declaration or full standard-library runtime closure.

Fresh follow-up probes reach explicit P8 missing-binding diagnostics for
Data.Int.fromStringAsImpl and Data.String.CodeUnits._charAt. Those target-library
implementation boundaries remain open; this compiler repair does not replace
them with successful-looking placeholders.
