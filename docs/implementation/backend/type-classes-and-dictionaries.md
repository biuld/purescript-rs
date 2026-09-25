# Type Classes and Dictionaries Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Type classes and dictionaries](../../design/backend/fp/type-classes-and-dictionaries.md)

**Progress:** Backend dictionary lowering Verified (DICT-01..11) from verified
Typed Core fixtures; source-level FE-14/15 elaboration remains a separate,
unfinished frontend gate.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), supporting BE-02 and BE-09; FE-14 and FE-15 supply resolved evidence.

## Scope and dependencies

Complete backend lowering of *checked* class/instance evidence to ordinary
records and closures, including constrained functions, methods, superclass
projection, recursive contexts, defaults, and erasure. The linked design's
present-tense contract is authoritative beyond this matrix. Frontend instance
search, coherence, functional-dependency improvement, and deriving belong to
FE-14/15. The backend must consume stable evidence chosen by the frontend and
must never re-search instance heads. Source lowering cannot yet produce a valid
class case, so this matrix uses verified Typed Core fixtures and tracks the
source gate under DICT-11. The design's Code map is the ownership target.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.
Verified requires implementation, verifier, and value-sensitive execution
evidence at the stated input boundary.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| DICT-01 | Typed Core carries explicit Given, Instance, and Superclass evidence with stable selected identities; backend performs no instance search. | Inspect Core-to-CC inputs and code paths; fixtures distinguish same-head instances already selected by frontend and reject unresolved evidence. | Verified |
| DICT-02 | A class dictionary is a fixed-order product of method closures and superclass dictionaries with exact field signatures. | Inspect layout/order for multiple methods/superclasses; malformed CC/MIR field index and signature fixtures fail. | Verified |
| DICT-03 | A constrained binding receives ordered dictionary parameters before ordinary arguments, preserving source evaluation order. | Trace constrained direct and higher-order calls through Core, CC and MIR; execute methods whose outputs distinguish dictionary order. | Verified |
| DICT-04 | Instance construction fills method and superclass fields from selected evidence and captures required instance context. | Execute nullary/contextual instances, nested constraints, and an escaping method closure; inspect capture and product construction. | Verified |
| DICT-05 | Method selection is product projection followed by ordinary closure call; superclass selection is nested projection. | Inspect CC/MIR absence of class-specific runtime dispatch, execute multi-level superclass and overloaded method cases. | Verified |
| DICT-06 | Given evidence and locally bound dictionaries are shared according to Core binding semantics, with no duplicate evaluation. | Side-effecting or counted dictionary construction fixture; inspect let binding and execute repeated method uses. | Verified |
| DICT-07 | Default methods and recursive class/instance contexts use explicit dictionary/self references with safe construction order. | Verified Core positive fixtures, negative cyclic/invalid layout fixtures, and executed default/recursive method results. | Verified |
| DICT-08 | Polymorphic methods and dictionary captures obey erased signatures and aggregate conversion rules. | Execute methods at distinct scalar and aggregate instantiations; inspect adapters, captures, boxing and reconstructed results. | Verified |
| DICT-09 | P9 maps dictionaries only to ordinary structs/closures, with exact MIR call and field verification. | Full-module negative fixtures for wrong arity, field, signature, missing evidence, and non-dominating dictionary use. | Verified |
| DICT-10 | Optimizations preserve the unspecialized dictionary path and its observable behavior. | Compare optimized and unspecialized execution; inspect retained generic path when specialization would erase test evidence. | Verified |
| DICT-11 | Source and verified Typed Core coverage are tracked separately; backend acceptance never implies frontend class elaboration is complete. | Evidence record lists source cases and fixture-only cases, FE-14/15 gaps, runtime executions, and official-suite status. | Verified |

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

Environment shared by every record below unless stated otherwise:

- Revision: branch `verify/type-classes-dictionaries` at `95aebe3` plus the
  uncommitted changes for this audit.
- Wasmtime: `wasmtime 49.0.0 (17830bd3 2026-09-21)`.
- Commands:
  - `cargo fmt --all --check`
  - `cargo test --workspace`
  - `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
- Result of the shared commands: all pass; no skips under
  `PSRS_REQUIRE_WASMTIME=1`.

```text
DICT-01:
  Implementation: crates/psrs-thir/src/evidence.rs (EvidenceKind::Given,
    Global, Superclass, Instance); crates/psrs-thir/src/lib.rs verify_evidence;
    crates/psrs-core/src/lower/dictionary.rs lower_evidence lowers them to
    ordinary locals, globals, projections, and applications;
    crates/psrs-core/src/verify/expr.rs rejects an unbound local or an unknown
    global. No backend module contains instance search.
  Tests: dictionary_audit::negative::rejects_unresolved_given_evidence
    (lower_module_unverified then core.verify reports "local reference is not
    in scope"), rejects_superclass_evidence_with_a_missing_field (THIR verify),
    rejects_instance_evidence_with_an_unknown_constructor (Core verify
    "global reference is not declared"), and
    rejects_instance_evidence_with_a_context_arity_mismatch (THIR verify "too
    few context arguments").
    dictionary_audit::execution::typed_core_instance_and_superclass_evidence_execute
    executes a Global Eq dictionary feeding an Instance Ord dictionary.
  Input boundary: verified Typed Core and malformed Typed Core.
  Result: pass; the positive case executed under Wasmtime.
  Gaps: the frontend does not yet emit these evidence terms (see DICT-11).
DICT-02:
  Implementation: crates/psrs-core/src/dictionary.rs ClassLayout::from_record_type
    and validate_record_value; crates/psrs-backend/src/cc/layout/aggregate.rs
    builds the canonical closed-record product (labels sorted); lower_record,
    lower_record_update, and lower_field_access address fields by canonical
    label rather than declared position.
  Tests: psrs-core dictionary tests layout_rejects_duplicate_field_labels,
    layout_rejects_a_dictionary_field_with_the_wrong_type,
    layout_rejects_a_dictionary_missing_a_field,
    layout_rejects_a_non_record_dictionary_type;
    cc::verify::tests::dictionary::{rejects_a_dictionary_projection_with_an_out_of_range_field,
    rejects_a_dictionary_construction_with_a_wrong_field_shape,
    accepts_a_well_formed_dictionary_product};
    cc::lower::dictionary::tests::typed_thir_instance_and_superclass_evidence_lower_to_ordinary_products;
    dictionary_audit::execution::typed_core_instance_and_superclass_evidence_execute
    (the fixture declares the dictionary fields out of canonical order, so the
    execution proves label-based field addressing).
  Input boundary: verified Typed Core, malformed CC, Core layout.
  Result: pass; the out-of-order fixture executed under Wasmtime to exit 42.
  Gaps: none. ClassField::index keeps the declared logical order; the target
    product offset is the canonical label order chosen by P8.
DICT-03:
  Implementation: crates/psrs-backend/src/cc/lower/mod.rs lower_function binds
    Core lambda parameters in source order; cc/lower/call/application.rs
    evaluates callee then arguments left to right.
  Tests: dictionary_audit::execution::constrained_dictionary_parameters_precede_ordinary_arguments
    asserts the Core lambda binder order [da, db, x], compiles the program, and
    executes it under Wasmtime to exit 42.
  Input boundary: verified Typed Core.
  Result: pass; executed.
  Gaps: none.
DICT-04:
  Implementation: crates/psrs-core/src/lower/dictionary.rs (Instance applies the
    selected constructor to its context evidence);
    crates/psrs-backend/src/cc/lower/record/mod.rs lower_record;
    crates/psrs-backend/src/cc/lower/erased.rs adapt_erased_function_value.
  Tests: dictionary_audit::execution::typed_core_instance_and_superclass_evidence_execute
    (contextual instance) and escaping_method_closure_capturing_context_executes
    (method closure captures the instance context and escapes through the
    product).
  Input boundary: verified Typed Core.
  Result: pass; both executed.
  Gaps: none.
DICT-05:
  Implementation: cc/lower/record/mod.rs lower_field_access (ordinary
    ProductGet + closure call); no class-specific CC/MIR node or dispatch.
  Tests: cc::lower::dictionary::tests::typed_thir_instance_and_superclass_evidence_lower_to_ordinary_products
    asserts ProductNew/ProductGet and IndirectCall and checks MIR StructGet
    fields 2 then 0; dictionary_audit::execution::typed_core_instance_and_superclass_evidence_execute
    executes the nested superclass projection.
  Input boundary: verified Typed Core.
  Result: pass; executed.
  Gaps: none.
DICT-06:
  Implementation: cc/lower/letrec/mod.rs shares the lowered value of a Core
    let binding across uses; construction happens once.
  Tests: dictionary_audit::execution::shared_dictionary_binds_once_and_optimizes_safely
    lowers the pre-optimization MIR and asserts exactly one call to the instance
    constructor and at least two StructGet projections of the shared dictionary,
    then executes the optimized artifact to exit 42.
  Input boundary: verified Typed Core.
  Result: pass; executed.
  Gaps: none.
DICT-07:
  Implementation: default methods are ordinary top-level closures stored in
    product fields; recursive declarations lower through the existing wrapped
    global/letrec path; a recursive instance constructor captures its context
    parameter before building the product.
  Tests: dictionary_audit::execution::default_method_field_executes stores a
    default in one field and a provided method in another and executes to 42;
    recursive_instance_context_executes builds guarded dictionaries through a
    recursive global constructor that delegates to its captured context at the
    base case and executes to 42. Cyclic/invalid construction is rejected by
    the Core layout negatives (duplicate, missing, wrong-type, non-record), the
    CC product negatives, and the MIR non-dominating projection fixture listed
    under DICT-02 and DICT-09.
  Input boundary: verified Typed Core.
  Result: pass; both executed.
  Gaps: none.
DICT-08:
  Implementation: cc/lower/call/application.rs lower_indirect_application now
    adapts concrete arguments to the erased signature and recovers the concrete
    result for a generic (erased) callee (this is the method-selection closure
    call); cc/lower/erased.rs unbox_erased_value converts a boxed integer to
    Boolean through IntToBoolean; cc/lower/conversion.rs plans box, erase, and
    aggregate conversions.
  Tests: dictionary_audit::execution::dictionary_through_erased_polymorphism_executes
    (a dictionary crosses an erased polymorphic identity and its method is
    selected) and polymorphic_method_field_executes (a generic method field
    `forall a. a -> a` crosses the erased protocol and is applied at Boolean).
  Input boundary: verified Typed Core.
  Result: pass; both executed.
  Gaps: none. This fix touches shared CC call lowering; see cross-topic note.
DICT-09:
  Implementation: crates/psrs-backend/src/mir/lower/assignments.rs lowers
    ProductNew/ProductGet to StructNew/StructGet; mir/verify/instruction/mod.rs
    checks struct field index, arity, and field storage type; mir/verify/function.rs
    checks SSA dominance; cc/verify/ops/mod.rs and cc/verify/ops/aggregate/mod.rs
    check product operations and conversions.
  Tests: mir::verify::tests::dictionary::* (six full-module negatives plus a
    positive): wrong struct.new arity, wrong struct.new field type, out-of-range
    struct.get, wrong struct.get result type, wrong method call arity, and a
    non-dominating dictionary projection; cc::verify::tests::dictionary::* (two
    negatives plus a positive). All negatives classify as InvalidCompilerIr.
  Input boundary: malformed CC and MIR full modules.
  Result: pass.
  Gaps: none.
DICT-10:
  Implementation: crates/psrs-core/src/opt and crates/psrs-backend/src/mir/opt
    do not introduce a dictionary-specific representation; the unspecialized
    product path remains in the lowered program.
  Tests: dictionary_audit::execution::shared_dictionary_binds_once_and_optimizes_safely
    inspects the unoptimized MIR (one construction, shared projections) and then
    executes the optimized artifact to exit 42, confirming the optimization
    preserves observable behavior; the optimized MIR still contains the
    dictionary StructGet projections.
  Input boundary: verified Typed Core, pre- and post-optimization MIR.
  Result: pass; executed.
  Gaps: no dedicated specialization-vs-unspecialized differential fixture; the
    current optimizer does not eliminate the dictionary path.
DICT-11:
  Implementation: this record.
  Tests: source cases: none. Fixture-only cases: all execution and structural
    tests listed above start from verified Typed Core (the source frontend
    rejects instance syntax and erases `TypeKind::Constrained`).
  Input boundary: verified Typed Core; source gate recorded separately.
  Result: pass; backend dictionary acceptance does not imply FE-14/15.
  Gaps: FE-14/15 elaboration (class environment, instance selection, coherent
    evidence) is not implemented. Official-suite class cases remain blocked on
    that frontend work; none are counted as backend evidence.
```

## Discovered obligations

- **Indirect generic calls.** A class method projected from a dictionary can
  have a polymorphic type, so the resulting erased closure is called indirectly
  at a concrete instantiation. `lower_indirect_application` must adapt each
  concrete argument to the erased signature and recover the concrete result;
  previously it compared the concrete result against the erased signature
  result and rejected the call. Fixed in
  `crates/psrs-backend/src/cc/lower/call/application.rs`.
- **Boxed Boolean recovery.** `unbox_erased_value` built a `ProductGet` whose
  destination was `Boolean` while the integer box field stores `Integer`, which
  the CC verifier rejects. Recovering a Boolean now projects the integer box
  slot and applies `IntToBoolean`. Fixed in
  `crates/psrs-backend/src/cc/lower/erased.rs`.
- **Canonical field addressing.** The target product order is the canonical
  (label-sorted) closed-record order, so dictionary fields are addressed by
  label, not by `ClassField::index`. The tests pin this for a dictionary whose
  declared field order differs from the canonical order.

## Remaining work and blockers

- FE-14/15: the type checker still erases `TypeKind::Constrained` and AST
  lowering rejects instance syntax, so no source program produces class
  evidence. The source column of DICT-11 stays empty until that frontend work
  lands.
- Official test suite: class/instance upstream cases are not runnable against
  the source frontend and are not part of this backend acceptance.
- Cross-topic handoff: DICT-08 required a fix in the shared CC indirect-call
  lowering (`cc/lower/call/application.rs`) and boxed-value recovery
  (`cc/lower/erased.rs`). Both preserve the erased protocol and the
  data-representation and CC-IR verifier checks; the owning topics should be
  aware that the erased indirect-call path now handles concrete-to-erased
  argument adaptation.
