# Backend Optimization

Optimization is a cross-cutting concern of the backend pipeline, not a new
representation or target. It has two homes:

| Document | Stage | Input and output | Owns |
| --- | --- | --- | --- |
| [core.md](core.md) | P7 | Typed Core → Typed Core | Source-type-aware simplification and specialization |
| [mir.md](mir.md) | P10, before structuring | MIR → MIR | Representation-preserving CFG and instruction optimization |

P8 and P9 perform required lowering, including administrative bindings,
closure conversion, layout planning, and ABI adaptation. They may simplify as
part of those conversions but are not independent optimization IRs. P10's Wasm
structurer and P11's encoder consume optimized MIR; neither chooses a runtime
representation or performs semantic optimization.

Every optional optimization must preserve program results, trap behavior,
evaluation order, and the order and number of observable calls unless a proof
establishes a stronger purity property. Each pass takes and returns the same
representation, verifies its input and output, keeps source spans on retained
operations, and can be disabled without changing acceptance or behavior.

The [IR boundary contract](../00-ir-boundaries.md) owns stage transitions;
[effects](../fp/effects.md) owns the effect semantics that constrain passes;
[MIR](../fp/mir.md) and [functional core](../../frontend/semantics/functional-core.md) own their
respective representation invariants.
