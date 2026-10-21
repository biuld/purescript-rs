# DEC-24: Effect.Exception Error representation

**Status:** Accepted, 2026-10-21\
**Feature:** [F-02](../feature/F-02-portable-programs.md)

## Context and constraints

The official synchronous exception API must execute on the Wasm/WASI target
while preserving exports, signatures and its non-orphan Show Error instance.
The existing nullary opaque foreign datatype convention denotes a WIT resource
handle, whereas Error is a GC-managed language value. The target has no JS Error
object or JavaScript stack. Compiler lookup by source module/function name and
successful-looking foreign placeholders are disallowed by source fidelity.

## Decision

Keep Error in Effect.Exception as a constructor-hidden newtype of String.
Retain its Show instance in that module. Replace its native foreign bodies with
ordinary library definitions at the original signatures. Use exactly two new
checked runtime primitives for throw/catch over one compiler-owned
psrs:source-error tag carrying a GC String reference. Error construction,
inspection and wrappers remain ordinary PureScript.

Retain only the message. Name returns Error and stack returns Nothing. Show
implements the official no-stack String(error) fallback, including Error for
an empty message. Catch selects source-error only. The budget's distinct
stack-exhaustion exception and engine traps propagate; JavaScript's catchable
stack overflow is a documented divergence. Source stack capture is a future
shadow-call-stack topic.

## Consequences

Public Error stays abstract without an external resource table or an opaque-type
layout exception. Signature checks happen before erasure and the message stays
GC-managed throughout unwinding. This is an explicit source representation
adaptation, audited and tested in the independent package, following the
constructor-hidden Effect.Ref precedent. Consumers using unsafe coercion have
no promise of JS Error-object identity or property inspection. The target cannot
claim stack-bearing JS Show equality. The ordinary try wrapper and MonadError
instances compose without additional compiler intrinsics.

The [design](../design/backend/fp/effect-exception.md) owns the current contract;
the [checklist](../implementation/backend/effect-exception.md) owns acceptance.
