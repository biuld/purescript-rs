# Native Effect and ST: Proposed Acceptance Checklist

**Status:** Design only; every requirement is Unverified.  
**Design:** [Native Effect and ST over algebraic handlers](../../design/backend/fp/native-effect-and-st-handlers.md).  
**Date:** 2026-10-11.

This is a new proposal after compiler protocol removal. Historical State-based
execution evidence is not evidence for this representation. The independent
package and normal compiler lock have not been migrated by this design task.

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| NE-01 | Public source fidelity | Pinned source audit; public exports/signatures/roles/classes/instances unchanged; native replacements explicitly justified | Unverified |
| NE-02 | Shared Effect/ST representation | Generic source-field and representation evidence; executed official ST.Global.toEffect with persistent references and multiple result types | Unverified |
| NE-03 | Inert construction and repeat execution | Public native API oracle for operation counts, strict factory arguments, delayed callbacks, and fresh allocations on repeated runs | Unverified |
| NE-04 | Heterogeneous request typing and identity | Int/String/function/record cells in one program; rank-N transports, in-place writes across polymorphic boxing, read/write optimization order, and malformed generic representation rejection | Unverified |
| NE-05 | Official Ref semantics | Aliasing, writes, read/modify' callback count and result, re-entrant unsafe callback behavior, and official oracle comparisons | Unverified |
| NE-06 | Self-referential initialization | Callback executes once; published self-reference works; unpublished read traps under the documented target adaptation | Unverified |
| NE-07 | ST region isolation | Official compiler differential cases for escaping references/actions, cross-region use, nominal Coercible rejection, constructor scope, and host-program injection | Unverified |
| NE-08 | Safe mutable-array isolation | Thaw/freeze copy behavior; repeated writes; callbacks; growth and alias transport; explicit unsafe sharing distinguished from safe APIs | Unverified |
| NE-09 | Deep stack-safe execution | Mandatory Wasmtime long public loops, MonadRec, bind chains, mixed native requests, and pure generic controls | Unverified |
| NE-10 | Native host handler behavior | Actual Console/Clock/Arguments/resource and other supported service execution; exact output/value/call order and lifetime observations | Unverified |
| NE-11 | Ordinary command/unsafe boundary | Configured runner executes once and returns zero; unsafePerformEffect repeats; traps propagate; no special Effect entry discovery | Unverified |
| NE-12 | User effects interpret into native Effect | Handler constructs native requests without executing early; residual operation rows are preserved or rejected appropriately | Unverified |
| NE-13 | Compiler neutrality | No State protocol, Effect discovery, native operation enum, hidden shape inference, or effect-specific optimizer exemption; generic regression evidence | Unverified |

Implementation order follows semantic dependency: validate shared generic
representation and typed requests; implement native interpretation and region
boundaries; adapt public modules with fidelity evidence; execute complete API
conformance; only then consider a coherent package pin. Native continuation
support remains an independent decision and validation track.

No runtime tests or differential cases were executed for this proposal. Record
exact compiler/package identities, commands, results, and skips for subsequent
evidence. Do not label a requirement Verified from this design text or a test
that exercises the withdrawn implementation.
