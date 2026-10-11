# Algebraic Effects: Proposed Acceptance Checklist

**Status:** Design only; all requirements below are Unverified.  
**Design:** [Library-owned algebraic effects and handlers](../../design/backend/fp/algebraic-effects-and-handlers.md).  
**Decision:** [DEC-19](../../decision/DEC-19-compiler-generated-resumable-frames.md)
selects compiler-generated resumable frames as the generic handler/interruption
mechanism.  
**Date:** 2026-10-11.

This checklist records evidence required for a future implementation. Creating
it does not resume the withdrawn State migration or select native Effect/ST
representation. Compiler cleanup and runtime build-tool changes are separate
work. The `AC-*` obligations are **required**, not conditional: DEC-19 selected
the generic control mechanism, implemented as compiler-generated resumable
frames rather than target stack switching.

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| AE-01 | Existing PureScript syntax and type system express computation rows and handlers | Pinned upstream source; official `purs` accept/reject comparison on positive and negative library cases | Unverified |
| AE-02 | Handler removes its slot and preserves the residual row | Missing operation, wrong result type, residual row at pure boundary, and wrong-kind rejection cases | Unverified |
| AE-03 | Compiler does not recognize concrete effect/library identities | Code review and equivalent behavior for renamed ordinary operation modules; no State/Effect protocol hooks | Unverified |
| AE-04 | Construction is inert and interpretation preserves strict argument behavior | Operation count/order, strict factory arguments, ignored operation results, and repeat interpretation observations | Unverified |
| AE-05 | Pure State interpretation has correct value threading | Write/read result 42, nested handlers, independent labelled instances, and forwarded requests | Unverified |
| AE-06 | Deep handlers forward and resume correctly | Nested handler order, handled requests after resumption, and outer handling of clause requests | Unverified |
| AE-07 | Generic representation composes with handlers | Captures, rank-N interpreter arguments, polymorphic ADT/record fields, higher-order calls, and signature adapters | Unverified |
| AE-08 | Deep interpretation is stack safe | Actual Wasmtime execution of long bind chains, nested requests, and loops; compare observable results with pinned upstream oracle | Unverified |
| AE-09 | Generic optimization preserves observable behavior | Mandatory execution with ignored results, traps, callbacks, and pure newtype controls; no effect-specific optimizer exemptions | Unverified |
| AE-10 | Source fidelity is preserved | Independent package audit with pinned revisions; every target adaptation has a concrete representation reason | Unverified |
| AE-11 | Host interpretation uses a defined native IO boundary | Separate selected ABI design and actual host behavior evidence; a pure handler test is insufficient | Unverified |

The generic control mechanism selected by DEC-19 adds these required
obligations. They remain Unverified; library encoding alone does not satisfy
them:

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| AC-01 | Typed control API is compatible with the language decision | Reviewed DEC-03 amendment (DEC-19) and complete suspension/delimiter/resumption contracts | Unverified |
| AC-02 | The selected representation can capture and resume | Actual target execution of resumable frames at supported boundaries, stack safety, and capability gating; no silent fallback | Unverified |
| AC-03 | Resumption multiplicity and lifetime are enforced | Double resume, re-entrant resume, cancellation, non-resuming clauses, and expired-boundary tests | Unverified |
| AC-04 | Captured values remain valid | GC during suspension, polymorphic capture recovery, foreign buffer/resource lifetime tests, and unsupported capture rejection | Unverified |
| AC-05 | Native execution preserves selected handler semantics | Deep/shallow boundary tests; distinguish one-shot rejection from permitted multi-shot behavior; suspension is not optimized away | Unverified |

No runtime or differential validation was run for this design. Record future
evidence with compiler/package identities, commands, actual outputs, and any
skips. Do not infer topic completion from existing generic compiler tests.
