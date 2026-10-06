# Standard-library source fidelity

The vendored standard library preserves the official PureScript source contract.
The only permitted semantic differences have a concrete Wasm/WASI target or
DEC-16 Unicode scalar/UTF-8 representation justification. This policy governs
both new imports and repairs of the existing library.

## Upstream provenance

Record the upstream repository URL, exact release tag, commit, and source hash
for every package/module. Dependency ranges do not identify a reproducible
baseline. Include all package source modules; an omitted module is an explicit
gap, not an implicitly supported subset. Audit byte-level differences and
review changes to APIs and behavior separately.

Official vendored `.purs` files retain upstream length even when they exceed
500 lines. The repository's 500-line rule continues to apply to maintained
compiler and tooling source. Do not split official modules to satisfy that rule.

The initial complete audit and pinned comparison baselines are recorded in
[the source audit](../implementation/stdlib/vendor-audit-2026-10-06/report.md).
That audit describes revision 67369ba, including defects; it is not an approved
patch manifest. Its repeatable inventory tool is
[audit-stdlib-vendor.py](tools/audit-stdlib-vendor.py).

The [restoration checkpoint](../implementation/stdlib/vendor-restoration-2026-10-06/report.md)
records restored sources, remaining target adaptations, and missing foreign
implementations separately from the historical defects.

## Preserve ordinary PureScript

Preserve official pure functions, type signatures, exports, type roles, classes,
instances, and modules. Do not remove a declaration because the compiler cannot
handle it. Do not eta-expand instance methods, replace a valid combinator,
change a record signature, or simplify a module to work around inference,
resolution, deriving, closure, or layout defects. Fix the owning compiler stage
and validate the unchanged source instead.

Compiler-provided interfaces must have one declared semantic owner and validate
their public source contract. A primitive implementation must not arise from a
function-name heuristic, inferred arity, or a fabricated source body.

## Foreign implementations and target adaptations

An official foreign declaration states the public type; the target implements
that contract through an explicit primitive or host binding. The compiler must
retain checked binding identity and type evidence through lowering. Where a
binding is not implemented, compilation or linking reports unsupported target
support. No recursive equation, arbitrary constant, empty result, or permissive
fallback can stand in for the implementation.

Each deliberate target adaptation records:

1. The pinned original declaration and exact source difference.
2. The target requirement, with its governing design or decision.
3. The implementation owner and preserved source/API guarantees.
4. Behavior evidence for normal, boundary, and failure cases.
5. Any deliberately different observable behavior and remaining obligations.

WASI routing and a declared trap-based failure protocol can justify changes to
host I/O and exception handling. They do not justify deleting comparison
assertions, tuple instances, console Show wrappers, or other pure APIs. UTF-8
storage does not justify removing Show instances or approximating unrelated
numeric rendering. Target limitations must be recorded without redefining
official support around the implemented subset.

## Verification and reporting

Keep these acceptance layers separate:

- Source fidelity: complete inventory, upstream hashes, exact diffs, and reviewed
  target adaptations, including omitted module/API checks.
- Compilation: the restored source closure passes the compiler, with missing
  support reported at its actual owning stage.
- Runtime: programs actually call the APIs and assert returned values, output,
  mutation, sequencing, and failure behavior under mandatory Wasmtime.
- Foreign behavior: each implemented binding has type/ABI checks and observable
  behavior comparisons with the official implementation or its explicit target
  contract. Keep DEC-16 differences visible in Unicode cases.

An import-only program with `main = 0` proves none of the runtime assertions.
Dead-code elimination can hide missing implementations and layouts. A runtime
scoreboard without output goldens proves execution without traps, not complete
semantic agreement. Missing or skipped execution is unverified.

Follow [the compiler iteration SOP](compiler-iteration-sop.md), use focused
tests for each restored interaction, and capture comparable compile diagnoses.
Update roadmap measurements only after the required full scoreboard run. Do not
claim completion while any required API, binding, source-restoration obligation,
or behavior evidence remains missing.
