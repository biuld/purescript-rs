# DEC-21 — Control Machine Construction and Verification Boundary

**Status:** Accepted
**Date:** 2026-10-14
**Governed by:** [F-02](../feature/F-02-portable-programs.md),
[DEC-16](DEC-16-scalar-strings-and-utf8-storage.md),
[DEC-17](DEC-17-representation-and-evidence.md) and
[DEC-19](DEC-19-compiler-generated-resumable-frames.md).
**Design:** [Control machine](../design/backend/fp/control-machine.md),
[delimited control](../design/backend/fp/delimited-control-and-resumable-frames.md)
and [handler scopes](../design/backend/fp/control-elimination-and-handler-scopes.md).
**Review:** [Delimited control lowering review, 2026-10-14](../implementation/backend/delimited-control-lowering-review-2026-10-14.md).

## Context and constraints

P8 currently expands each control primitive into ordinary CC plus a
`ControlEvidence` witness. The CC and MIR machine verifiers then recover the
machine's meaning from emitted projections, calls, casts, aliases, dispatch
paths and private-cell writes. Shared relations reduce duplication, but the
recognizers are coupled to the exact instruction forms that survive
optimization: every change in expansion or in a generic rewrite becomes a new
recognition obligation. The witness proves a factory occurrence, not the whole
generated executor action.

There is no single named authority for the machine schema or for the set of
required transition actions. Frame cases and fields are declared positionally
and their offsets recur in layout, lowering and verification; queue, context,
extent and status roles are inferred from helper signature positions. Machine
meaning and ordinary IR validity are therefore validated by the same mechanism,
which does not by itself establish semantic preservation across optimization.

The machine must remain a software-frame implementation of
[DEC-19](DEC-19-compiler-generated-resumable-frames.md): a persistent
sequencing spine, one extent per delimiter (with a permitted tail-finalization
coalescing group), and a flat compiler-generated guest loop. The source surface
remains the monadic one selected by [DEC-20](DEC-20-typed-control-elimination.md).

## Decision

Machine semantics are discharged during P8 construction at an explicit boundary,
not reconstructed after optimization.

1. One **named machine schema** owns the machine's cases, fields, helper roles
   and fixed signatures. Physical layout planning, P8 generation and every
   verifier consume it. Semantic roles are not recovered from helper parameter
   offsets or variant field positions.
2. P8 builds a temporary, effect-agnostic **checked construction plan** with
   named operands, selected cases, projections, ordered
   allocation/read/write/check/close/apply operations and exactly one terminal
   outcome (`Advance`, `Publish` or `Reject`). The plan retains Core's checked
   domain identities at source and callback boundaries and is checked against
   the operational semantics before emission.
3. A **boundary checker** compares the emitted ordinary CC actions and their
   actual exits against the checked plan, accounting for guards and exceptional
   prefixes. An action identity tag alone is not a body proof.
4. After the boundary, ordinary P9 representation lowering and P10 optimizations
   are part of the compiler's trusted implementation. They must preserve
   ordinary call, mutation, trap and identity semantics and are validated by
   their ordinary IR invariants and behavior-preservation tests. SSA/layout
   validity is not treated as a semantic-preservation proof, and no certificate
   may authorize instructions modified after it was issued.
5. P9 keeps the representation, capture, CFG and loopification contracts it
   already owns. Facts explicitly discharged at the boundary are not re-derived;
   P9 checks the facts it still owns rather than re-recognizing machine actions.

This changes the cross-stage assurance contract, not the selected runtime
semantics. The plan is not a whole-program Control IR, a runtime interpreter or
a source effect system. No runtime type reflection or concrete operation name is
introduced; boundary freshness and source/checkpoint provenance are obligations
of the plan, not annotations the emitter may assert.

## Consequences

- The emitter/boundary checker and the ordinary P9/P10 passes become trusted
  implementation, as for ordinary closures and mutable data. This is not a
  formal correctness proof and no pass-level equivalence proof is claimed.
- Existing CC/MIR semantic checks remain in force during migration. A check may
  be removed only after every positive and malformed-input case is mapped to its
  new owner and the newly trusted transformation has optimized Wasm behavior
  evidence. Negative coverage is not deleted to make removal easier.
- The schema and registry are independently checked against the operational
  semantics; generating the emitter and its checker from one unchecked field
  list would only duplicate the same defect.
- Source typing, payload adapters and closure protocols keep their existing
  owners. Foreign borrowed-resource lifetime and caught-host-trap recovery remain
  separate unresolved contracts and are not discharged here.
- The required sequencing representation keeps the precedent of *Reflection
  without Remorse*; this decision does not change queue shape, replay cost or
  Finish coalescing preconditions, which stay separately specified.
