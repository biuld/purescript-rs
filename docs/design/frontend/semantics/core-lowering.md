# THIR to Typed Core Lowering

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [Functional Core](functional-core.md),
[type inference](../type-system/type-inference.md),
[classes and evidence](../type-system/classes-and-evidence.md), and
[frontend boundaries](../00-ir-boundaries.md).

**Summary:** P6 converts verified THIR into the small Typed Core calculus. It
removes remaining high-level term distinctions, preserves checked types,
resolved identities, evidence, and source ranges, and verifies the Core module
that P7 and P8 consume. It does not choose runtime layouts or emit CC.

## Scope

This document owns the THIR-to-Core conversion and the handoff to P7. The
Core grammar and semantics are in [Functional Core](functional-core.md).
Earlier surface desugaring belongs to [P4](desugaring.md), and closure
conversion belongs to backend [CC IR](../../backend/fp/cc-ir.md).

## Background

THIR records checked source-level distinctions useful to diagnostics. Core
keeps a smaller expression set: ordinary functions, applications, typed lets,
constructors, records, arrays, conditionals, and normalized cases. Converting
at this boundary prevents backend passes from depending on `do`, guards,
overload search, or unresolved type expressions.

## Model

```text
lower_core : VerifiedTHIR -> Result<VerifiedCore, Diagnostics>
CoreOrigin = { source: SourceId, range: TextRange }
```

Every Core expression has a checked `TypeId` and an origin. Every reference
uses a stable resolved symbol or local ID. A type-class constraint has been
replaced by explicit dictionary parameters and evidence expressions before
Core verification. Core patterns are normalized constructor, record, variable,
or wildcard forms, not the full surface grammar.

## Design

P6 converts every THIR declaration and expression by an exhaustive mapping.
It erases type-only application and abstraction after recording quantified
variables at binding sites; it keeps newtype identity in metadata while using
the field representation later. It converts class evidence to ordinary Core
values, method selection to record projection, and constrained declarations
to dictionary parameters. It normalizes remaining pattern forms to Core's
smaller pattern grammar and preserves branch order and exhaustiveness
evidence.

External declarations retain source binding metadata in a side table. P6
checks that every source external appears exactly once there, but target WIT
resolution stays in P9. The entry symbol is selected by the driver from a
resolved declaration, never by textual matching in the backend.

Rejected alternatives: aliasing THIR as Core would leave syntax-only cases in
backend input; using a backend layout in Core would cross P9 ownership; and
dropping spans would lose diagnostics for later unsupported operations.

## Algorithms

```text
lower_module(thir):
    verify_thir(thir)
    convert and intern checked types, retaining quantified IDs
    register declarations, constructors, newtypes, and externals
    for each declaration: lower its expression and explicit evidence
    build external-binding projection and entry SymbolId
    verify_core(result)
    return result
```

Recursive declarations register IDs before bodies are lowered. Generated
dictionary binders receive fresh local IDs and preserve the constraint's
source origin. A branch is lowered only after P5 has checked a common result
type; a missing evidence term is a frontend error, not a backend placeholder.

## Code map

`crates/psrs-core/src/lower.rs` owns
`lower_module(thir: &psrs_thir::Module) -> Result<core::Module,
Vec<Diagnostic>>`. `pattern.rs` normalizes Core patterns; `link.rs` links and
prunes Core declarations after lowering; `verify/` checks types, scopes,
patterns, and source origins. `psrs-driver` supplies linked THIR modules and
entry selection. No `psrs-core` module imports CC, MIR, or Wasm types.

## Invariants and verification

No unresolved name, untyped expression, open constraint, source guard, or
layout token remains. Every Core type and symbol reference is valid; every
expression and pattern has the checked type and source origin required by the
Core verifier. The external side table agrees with source declarations. P7
and P8 may trust Core only after `Module::verify` passes.

## Worked example

For `identity :: forall a. a -> a; identity x = x`, P5 records the rigidly
checked signature and inferred body type in THIR. P6 emits one Core
declaration with quantified `a`, a typed lambda binder, a `LocalId` use of
that binder, and the original source ranges. It does not make an erased box or
a Wasm function type; those decisions occur in P8/P9.

## Boundaries and interfaces

P6 receives verified THIR and emits verified Core plus external metadata for
the backend. P7 may simplify Core while preserving this contract. P8 consumes
Core to produce CC through ANF and closure conversion. The driver retains the
source map for diagnostics across the boundary.

## Open questions and future work

Higher-rank types require Core binder and instantiation rules before P6 can
lower them. Open rows require a closed or explicitly abstract Core record
contract. These extensions must preserve the same P6 verifier boundary.

## References

- [Functional Core](functional-core.md),
  [frontend boundaries](../00-ir-boundaries.md),
  [frontend type-system design](../type-system/README.md), and
  [backend CC IR](../../backend/fp/cc-ir.md).
