# Handoff: Remove Compiler-Owned Effect and State Mechanisms

**Date:** 2026-10-11  
**Scope:** Compiler cleanup only; no replacement Effect design.  
**Status:** Instructions for the next agent. Removal has not been implemented.

## User decision and authority

The user explicitly requests removal of all compiler-side Effect-specific
logic and State analysis, including both the legacy Effect mechanism and the
new State dependency mechanism. The user explicitly rejects selecting an
ordinary thunk representation as part of this task: the replacement design
has not been decided.

This instruction supersedes the previous continuation order in
[library-owned effects](library-owned-effects.md) and the selected State
direction in the existing design documents. Do not resume those implementation
slices. The earlier implementation goal was paused; this handoff does not
resume it.

Do not introduce a new Effect representation, library thunk implementation,
State protocol, execution intrinsic, or alternate dependency analysis to make
the deletion appear complete. The deliverable is removal, with the remaining
boundaries and unsupported consumers stated honestly.

## Checkout and existing work

At handoff inspection, the compiler checkout is
`/Users/biu/Projects/purescript-rs`, branch `stdlib/vendor-core-libraries`,
HEAD `6b40b8d6` (`Record public Effect.Ref reference adapter evidence`). It has
substantial tracked and untracked work across Core, CC, MIR, driver tests, and
documentation. Inspect status and diffs again before editing.

The independent `/Users/biu/Projects/psrs-stdlib` checkout also has substantial
uncommitted work. Its development sources currently depend on `Prim.State`,
State-stepping runtime/host declarations, and the library State representation.
The compiler's normal lock still points to revision
`1e01310daa7e56de5ed379029d6a24dcf39e879c`, which uses the legacy mechanism.
Neither package is evidence of a compatible post-removal implementation.

Preserve unrelated work. Do not reset the checkout, drop stashes, revert the
entire migration commits, stage all paths, or rewrite history as a shortcut.
Those commits also contain useful generic runtime, linker, representation,
and optimizer changes. Do not commit, push, or change the normal stdlib lock
unless separately requested.

## Required removal

The paths below are investigation entry points, not an exhaustive deletion
list. Follow their callers, public fields, enum variants, verifier hooks,
configuration, tests, diagnostics, and documentation throughout the repository.

### Legacy Effect mechanism

Remove compiler ownership of the identity, representation, and operations of
the library type `Effect`:

- Driver discovery of `TrustedEffect`, trusted operation lookup, and lexical
  restrictions on `runEffect` in `crates/psrs-driver/src/program/effects.rs`.
- `EffectCompilation`, `EffectCommandEntry`, and other Effect-specific context
  passed through driver, Core, linker, and backend APIs.
- Synthesized `pure`, `bind`, runner, and trap implementations in
  `crates/psrs-core/src/effect/`, including operation supplies and lowering.
- Backend Effect-specific entry adaptation, suspension plans, import
  classification, and verification in `crates/psrs-backend/src/effects/`.
- Effect-specific callable policies, fixed-token conventions, opaque-type
  exceptions, and implicit host suspension based on Effect result types.
- Legacy `StateToken` expressions, representations, constant-zero lowering,
  and traversal/verification branches used only by that protocol.

Extract generic entry selection or reusable source diagnostics from mixed
modules before deleting their Effect-specific parts. Do not remove unrelated
entry selection merely because its current file is named `effects.rs`.

### State dependency mechanism

Remove the compiler's special interpretation of the State protocol:

- Core State signature classification, flow evaluation, scoped/captured token
  tracking, dependency graphs, stale-token/fork checks, and summaries in
  `crates/psrs-core/src/state/`.
- CC State execution plans, graphs, branch/capture analysis, and
  `StateCallProjection` in `crates/psrs-backend/src/cc/state/`.
- MIR State projection, inventories, immutable checked source, operand and CFG
  correspondence, and tail-transfer proof replay in
  `crates/psrs-backend/src/mir/state/`.
- State-specific fields on shared IR objects and all producer, transformation,
  verification, serialization, and linking hooks for those fields.
- State slots and State-aware conversion, capture, signature, result, and
  physical representation handling, including `UnprojectedState` contracts.
- State-stepping inference in WIT conformance, runtime bindings, reachable
  layout publication, and host call lowering. A function or record shape must
  not trigger hidden State protocol interpretation.
- Linker world-boundary construction and special runner facades in
  `crates/psrs-linker/src/world_boundary/` and their consumers.
- State-specific optimizer exclusions and eligibility checks. Review each
  mixed condition; preserve independent correctness constraints.

Also remove compiler-only facilities introduced to support this mechanism:
the `Prim.State` primitive catalog/typechecking rules, `RunWorld`/`RunRegion`
intrinsics and descriptors, and `StateExecutionBoundary` target handling.
Entry points include `psrs-hir/src/primitives/state.rs`,
`psrs-hir/src/intrinsic/state.rs`, and
`psrs-typecheck/src/typecheck/state.rs`.
Do not leave dormant registered intrinsics or special primitive types as a
compatibility route. An ordinary user/library declaration named State must
continue to receive ordinary language treatment.

## What must remain generic

"Effect-related logic" here means compiler ownership of the library Effect
abstraction and the State protocol. It does not mean removing the compiler's
ordinary ability to compile observable calls and mutable operations.

Preserve:

- Strict evaluation order, generic foreign calls, traps, and conservative
  optimizer treatment of observable or unknown calls.
- General effect summaries used by Core rewrites and MIR dead-code
  elimination, such as `psrs-core/src/opt/effects.rs` and
  `psrs-backend/src/mir/opt/effects.rs`. Remove protocol-specific cases inside
  them, not their generic safety rules.
- Ordinary newtype checking, roles, constructor visibility, generic callable
  storage, closures, captures, instantiation, adapters, and representation
  conversion.
- Generic direct/indirect tail calls, loop lowering, and argument remapping.
- Real runtime storage implementations, GC provider support, generic runtime
  linking, and checked source-value/ABI correspondence that is independent of
  State. Remove their State-shaped contract coupling; do not invent a new
  Effect ABI. If no generic replacement contract exists, report that missing
  interface rather than fabricate successful binding.
- Generic package-configured command runner invocation only if it uses
  ordinary function application and has no Effect/State-specific discovery,
  validation, or lowering. Existing State-dependent runner code is not a
  reason to preserve world-boundary machinery.

The uncommitted `opt/specialize/callable_fields.rs` guard is a separate generic
specialization workaround, not a State requirement. Review it independently.
Earlier focused runs passed nine tail-call tests and 24 Core optimizer tests;
these results predate removal and do not establish a complete adapter repair.
Do not remove the pure newtype deep-bind regression because its State variant
is being retired. The CLI binary was last built during a temporary experiment
with specialization disabled; rebuild before using it as evidence.

## Execution instructions

1. Inventory both mechanisms and all their consumers. Capture the current
   diff and a small relevant baseline without altering the index. Classify
   shared changes before removing anything.
2. Update the governing design and acceptance documents to state that the
   State direction is withdrawn and the replacement Effect representation is
   undecided. Existing normative text cannot remain an instruction to rebuild
   the removed machinery. Preserve dated evidence as historical evidence.
3. Remove both mechanisms across their owning stages and public interfaces.
   Remove obsolete registrations and configuration as well as implementation
   files. Keep stage contracts coherent after metadata removal.
4. Remove tests that exist solely to assert retired compiler protocols.
   Preserve independent semantic and generic compiler regression tests where
   their fixtures can remain ordinary source. Do not invent a new Effect
   implementation just to retain an old fixture.
5. Record affected package consumers and unresolved runtime/host interfaces.
   Do not modify the independent stdlib implementation or its public contracts
   as part of this cleanup. Missing support must remain an explicit failure.
6. Review the full resulting diff and report deletion coverage, generic code
   retained, checks executed, failures, and unresolved consumers.

## Validation and completion

Use focused crate checks/tests while iterating. The user previously requested
avoiding repeated full workspace runs. Run final integrated checks once the
removal stabilizes; if package-dependent tests cannot run or fail, report
their actual status and cause rather than disabling or reporting them green.

Required evidence:

- Searches and code review show no live legacy Effect synthesis, trusted
  identity path, State analyzer, dependency witness, execution intrinsic,
  special State representation, or State-stepping signature inference.
  Explain remaining textual matches instead of targeting zero occurrences of
  the English words "state" and "effect".
- Generic compiler checks and targeted runtime tests exercise ordinary
  closures/newtypes, observable call order, calls with unused results, trap
  preservation, mutable storage where a generic interface already exists,
  and the pure deep-bind/tail-call regression.
- Formatting and diff whitespace checks pass. Report the scope and result of
  workspace check/tests and clippy, including omissions and package blockers.
- Designs and the implementation ledger no longer present the removed
  protocols as current requirements or completed capabilities.

Completion means compiler ownership of both protocols is removed and all
remaining consumers and limitations are explicit. It does **not** mean a new
Effect mechanism exists or that either current stdlib package remains
executable. Designing that mechanism is a separate user decision.
