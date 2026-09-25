# Type Classes and Dictionaries Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Type classes and dictionaries](../../design/backend/fp/type-classes-and-dictionaries.md)

**Progress:** Unverified; audit existing implementation and tests first.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), supporting BE-02 and BE-09; FE-14 and FE-15 supply resolved evidence.

## Scope and dependencies

Complete backend lowering of *checked* class/instance evidence to ordinary
records and closures, including constrained functions, methods, superclass
projection, recursive contexts, defaults, and erasure. The linked design's
present-tense contract is authoritative beyond this matrix. Frontend instance
search, coherence, functional-dependency improvement, and deriving belong to
FE-14/15. The backend must consume stable evidence chosen by the frontend and
must never re-search instance heads. If source lowering cannot yet produce a
valid case, use verified Typed Core fixtures and track the source gate
separately. Use the design's Code map as the ownership target.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.
Verified requires implementation, verifier, and value-sensitive execution
evidence at the stated input boundary.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| DICT-01 | Typed Core carries explicit Given, Instance, and Superclass evidence with stable selected identities; backend performs no instance search. | Inspect Core-to-CC inputs and code paths; fixtures distinguish same-head instances already selected by frontend and reject unresolved evidence. | Unverified |
| DICT-02 | A class dictionary is a fixed-order product of method closures and superclass dictionaries with exact field signatures. | Inspect layout/order for multiple methods/superclasses; malformed CC/MIR field index and signature fixtures fail. | Unverified |
| DICT-03 | A constrained binding receives ordered dictionary parameters before ordinary arguments, preserving source evaluation order. | Trace constrained direct and higher-order calls through Core, CC and MIR; execute methods whose outputs distinguish dictionary order. | Unverified |
| DICT-04 | Instance construction fills method and superclass fields from selected evidence and captures required instance context. | Execute nullary/contextual instances, nested constraints, and an escaping method closure; inspect capture and product construction. | Unverified |
| DICT-05 | Method selection is product projection followed by ordinary closure call; superclass selection is nested projection. | Inspect CC/MIR absence of class-specific runtime dispatch, execute multi-level superclass and overloaded method cases. | Unverified |
| DICT-06 | Given evidence and locally bound dictionaries are shared according to Core binding semantics, with no duplicate evaluation. | Side-effecting or counted dictionary construction fixture; inspect let binding and execute repeated method uses. | Unverified |
| DICT-07 | Default methods and recursive class/instance contexts use explicit dictionary/self references with safe construction order. | Verified Core positive fixtures, negative cyclic/invalid layout fixtures, and executed default/recursive method results. | Unverified |
| DICT-08 | Polymorphic methods and dictionary captures obey erased signatures and aggregate conversion rules. | Execute methods at distinct scalar and aggregate instantiations; inspect adapters, captures, boxing and reconstructed results. | Unverified |
| DICT-09 | P9 maps dictionaries only to ordinary structs/closures, with exact MIR call and field verification. | Full-module negative fixtures for wrong arity, field, signature, missing evidence, and non-dominating dictionary use. | Unverified |
| DICT-10 | Optimizations preserve the unspecialized dictionary path and its observable behavior. | Compare optimized and unspecialized execution; inspect retained generic path when specialization would erase test evidence. | Unverified |
| DICT-11 | Source and verified Typed Core coverage are tracked separately; backend acceptance never implies frontend class elaboration is complete. | Evidence record lists source cases and fixture-only cases, FE-14/15 gaps, runtime executions, and official-suite status. | Unverified |

## Vertical execution order

1. Audit frontend evidence form, Core signatures, CC product/closure lowering,
   MIR verifier, optimizer, and runtime fixtures against the linked design.
2. Complete every evidence form and dictionary operation with malformed
   verifier cases; keep resolution ownership in the frontend.
3. Execute contextual, superclass, default, polymorphic, and optimized cases
   through the normal component path. Record fixture-only coverage explicitly.
4. Update D-04 and the evidence record; FE-14/15 and official-suite gates
   remain distinct from backend dictionary acceptance.

## Evidence record and completion rule

For each ID record code paths/functions, exact tests/assertions, input
boundary, commands, Wasmtime version, executions/skips, revision, and gaps.
Typed Core fixtures establish backend behavior only. Required runtime cases
use `PSRS_REQUIRE_WASMTIME=1`; skipped cases leave rows unverified. After Rust
edits run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused mandatory
runtime cases. Complete only when every row and all present-tense design
requirements are Verified.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
DICT-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. Existing dictionary-shaped product tests do not prove
all evidence forms or a source-level FE-14/15 path.
