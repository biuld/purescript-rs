# MIR Optimization

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [MIR](../fp/mir.md), [Core optimization](core.md),
[IR boundaries](../00-ir-boundaries.md), SSA dominance, and Wasm GC reference
types.

**Summary:** P10 optimizes verified MIR before Wasm structuring while keeping
P9's chosen value types, defined types, calling conventions, and ABI adapters.
It owns local CFG and instruction improvements with explicit effect, trap, and
memory rules. Each pass returns valid MIR and remains optional for correctness.

## Scope

This document owns MIR-preserving optimization and its pass-manager contract.
It does not choose layouts, specialize source types, create canonical ABI
adapters, or encode Wasm. Those responsibilities remain with P9, P7, P9, and
the [Wasm encoder](../wasm/encoding-and-structuring.md), respectively.
Control-flow structuring and tail-call mechanics are specified in
[control flow](../fp/control-flow-and-tail-calls.md).

## Background

SSA makes use-def chains, dominance, and local liveness explicit. This permits
constant propagation, dead-code elimination, and branch simplification without
recovering source syntax. MIR already fixes physical types, so an optimization
must preserve the type table and each operation's concrete calling convention.
Wasm traps and canonical ABI calls are observable: removing or moving them can
change a program even when its final scalar result is unchanged.

## Model

```text
MirPass = (Module, TargetCapabilities) -> Result<Module, Diagnostics>
InstructionEffects = { may_trap, reads_memory, writes_memory, may_call }
ConstantFact = Unknown | Constant(Scalar) | Overdefined
```

`may_call` includes direct, closure, and import calls unless a trusted summary
proves narrower behavior. The bounded inliner uses the complete body of a
known direct callee as that summary; it leaves import, closure, and nested calls
in place. Memory flags cover canonical ABI buffers and scratch state. An
instruction is *pure and total* only when all flags are false. GC
allocation and reads may be classified this way only when their operands and
bounds are proved valid and object identity is not observable; otherwise they
conservatively set `may_trap`.
The ordering relation is derived from the instruction sequence and CFG edges.
The constant lattice starts function parameters and unsupported definitions as
`Overdefined`; a block parameter becomes constant only when every reachable
incoming jump supplies the same known scalar. Cycles with unresolved inputs
remain `Unknown`, so the pass never guesses a loop value.

## Design

The P10 optimizer runs after P9 has planned the type table and lowered CC to
MIR, but before the structured Wasm form exists. Its baseline passes are
reachable-block pruning, constant propagation of total operations, branch and
switch simplification, local value forwarding, and dead pure-instruction
elimination. It may inline a small MIR function when it clones blocks with
fresh IDs, preserves call-site evaluation and traps, and leaves types and
calling conventions unchanged.

The optimizer never creates or deduplicates `DefinedTypeId`s, changes a field
layout, converts a boxed value to an unboxed calling convention, or rewrites a
WIT signature. Those are P9 representation decisions. Dictionary method
specialization at MIR may eliminate a known projection or direct a known call
without changing the dictionary's ABI; source-type-dependent specialization
belongs to P7.

The bounded inliner accepts only a single-block callee with no block parameters,
at most 16 instructions, and no nested call instruction. Across one module it
inlines at most 128 call sites and clones at most 1024 callee instructions in
total. These module-wide budgets are charged in function, block, and instruction
order. The growth charge is the number of body instructions cloned: replacing a
call with those instructions and one result copy increases the instruction
count by exactly the body length. If the next callee does not fit the remaining
growth budget, its call stays intact and later candidates are still considered;
once the call-site budget is exhausted, remaining calls stay intact. It clones
the body at the original call position, gives every cloned definition a fresh
value ID, and keeps the cloned operations' spans, traps, and memory effects in
order. Multi-block control flow, block parameters, and recursive call expansion
remain as calls.

P9 validates every external declaration before optimization. After the final
MIR pass, import projection uses direct calls, function references, and closure
construction in reachable optimized blocks; the ABI registry keeps the prevalidated
signatures and names. Dropping an unused import is module assembly, not a
change to its canonical ABI.

Rejected alternatives: optimizing encoded Wasm obscures source spans and MIR
dominance; re-planning layouts after each optimization blurs P9 ownership; and
classifying all allocations or memory loads as pure would permit trap removal.

## Algorithms

```text
optimize_mir(module, target):
    verify_mir_with_capabilities(module, target)
    module = inline_small_functions(module)
    verify_mir_with_capabilities(module, target)
    repeat:
        changed = prune_unreachable(module)
        verify_mir_with_capabilities(module, target)
        changed |= propagate_constants(module)
        verify_mir_with_capabilities(module, target)
        changed |= simplify_terminators(module)
        verify_mir_with_capabilities(module, target)
    until not changed
    module = forward_values(module)
    verify_mir_with_capabilities(module, target)
    module = eliminate_dead_pure(module)
    verify_mir_with_capabilities(module, target)
    module = project_reachable_imports(module)
    verify_mir_with_capabilities(module, target)
    return module

eliminate_dead_pure(function):
    compute uses from reachable instructions, terminators, and function.result
    remove an unused instruction only if it is pure and total
    repeat until no further pure definition becomes unused
```

Constant folding uses the semantics of [scalars](../fp/scalars-and-primitives.md):
32-bit wrapping and shifts, binary64 arithmetic, comparisons, `NaN`, and signed
zero. Signed division and remainder are folded only when the operands prove the
operation will not trap. Operations with unknown or possible trap behavior stay
in place. A known branch may be replaced with a jump after preserving every
instruction evaluated before its condition. Removed blocks may leave unused
types in P9's table; this is valid and avoids type-index churn.

## Code map

`crates/psrs-backend/src/mir/opt/` owns MIR optimization. `mod.rs` defines
`optimize(module: mir::Module, target: TargetCapabilities) -> Result<mir::Module,
Vec<BackendError>>` and verifies after each pass. `effects.rs` classifies
instructions; `cfg.rs` owns reachability and branch rewrites; `constants.rs`
owns scalar folding; `values.rs` owns forwarding and dead pure instructions;
`inline.rs` owns bounded MIR inlining; `imports.rs` projects prevalidated ABI
imports after optimization, retaining symbols used by reachable direct calls,
function references, or closure construction. The Wasm structurer consumes the resulting MIR and
does not call an optimizer internally.

## Invariants and verification

Every pass preserves valid SSA: unique definitions, dominance, block-parameter
arity and types, terminators, and source spans on retained operations. It
preserves the MIR type table and function signatures, the sequence of
remaining potentially effectful calls and memory operations along each executed
path, and the position of possible traps relative to those operations. A
bounded inline removes only a direct internal call whose complete body is
cloned in place; calls and effects inside that body remain ordered. The selected
capability profile still accepts every emitted instruction. Differential
execution compares optimized and unoptimized MIR through the same Wasm target,
including traps, effect traces, branches, and recursion.

Dead-code elimination treats `Function.result` as a use because the structurer
loads that value for the Wasm function result, independently of the return
terminator. Calls, memory operations, allocation, checked projections, casts,
and explicit trap instructions remain live even when their result is unused.

## Worked example

```text
B0: v0 = Constant true; v1 = Call log(v_before)
    Branch(v0, B1, B2)
B1: v2 = I32Add(2, 3); Return(v2)
B2: v3 = Call log(v_unreachable); Return(v_zero)
```

Branch simplification turns `B0`'s terminator into `Jump(B1)`, prunes `B2`,
and folds `v2` to `5`. It keeps the `log(v_before)` call in place. Import
projection removes an import used only by `B2` after pruning, while the
external declaration was already validated before the pass.

## Boundaries and interfaces

P9 supplies verified MIR, its fixed layout table, prevalidated ABI bindings,
and `TargetCapabilities`. P10 returns verified MIR to the structurer, which
converts the CFG to structured Wasm without a further representation decision.
P11 validates the final module and component. Tail-call loopification may run
as a MIR-preserving canonicalization before structuring, subject to the same
SSA and capability checks.

## Open questions and future work

Profile-guided inlining, loop rotation, and code-size heuristics need measured
budgets. A more precise effect analysis may permit additional load forwarding
or call elimination only after alias, trapping, and external behavior are
proved. Unboxing across call boundaries remains P9 work because it changes
representation and signatures.

## References

- Appel, *SSA is Functional Programming* (1998).
- [MIR](../fp/mir.md), [effects](../fp/effects.md),
  [control flow](../fp/control-flow-and-tail-calls.md), and
  [Wasm encoding](../wasm/encoding-and-structuring.md).

## Implementation notes

P10 runs verified MIR optimization in `crates/psrs-backend/src/mir/opt/` and
re-verifies after each pass. Its direct inliner enforces the 16-instruction
per-callee limit, the 128-call-site module limit, and the 1024-instruction
module-wide growth limit in deterministic function/block/instruction order.
Focused tests exercise both aggregate budget boundaries and verify that an
inlined body retains memory writes, a potentially trapping operation, memory
reads, and their source spans in order. The optimizer keeps calls whose bodies
are recursive, multi-block, over the per-callee limit, or do not fit the
remaining growth budget. Profile-guided profitability and optimized-versus-
unoptimized execution comparisons remain future verification work.
