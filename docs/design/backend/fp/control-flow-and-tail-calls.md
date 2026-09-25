# Control-Flow Lowering and Tail Calls

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [MIR](mir.md) and its SSA/CFG model, [CC IR](cc-ir.md);
dominance, natural loops, and the difference between reducible and irreducible
control-flow graphs; the WebAssembly structured-control and tail-call
instructions. Read [IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** MIR is an arbitrary control-flow graph, while WebAssembly is
structured, so this document specifies how the CFG is recovered as
`block`/`loop`/`if`/`br`/`br_table`, how multi-way pattern dispatch becomes a
`Switch` terminator and then a `br_table`, and how tail calls become
`return_call`/`return_call_ref` or a loop. It removes the temporary
`merge_block` structuring hint and makes the structurer total over reducible
graphs.

## Scope

This document owns MIR control-flow terminators, the structuring of a CFG to
structured Wasm, and the representation of recursion and tail calls. It does not
own runtime values, layouts, or calling conventions ([MIR](mir.md),
[data representation](data-representation.md)), the pattern decision that
produces `Switch` targets ([pattern matching](pattern-matching.md)), the module
skeleton and section encoding ([Wasm encoding](../wasm/encoding-and-structuring.md)),
or the capability flags themselves
([capability profile](../wasm/capability-profile.md)).

## Background

**Structured control flow.** WebAssembly has no arbitrary jumps. Its structured
control constructs are `block` (a branch exits at its end), `loop` (a branch
re-enters at its header), and `if`, each introducing a label; `br` and
`br_table` target labels by **relative depth**, counting outwards from the
branch. A `br` to a `block` label means "break"; a `br` to a `loop` label means
"continue". Because MIR blocks carry parameters and jumps carry arguments, the
lowering also copies jump arguments into the target block's locals, so the
structurer never needs Wasm stack typing or block results for parameters. This
keeps structuring independent of the multi-value work.

**Reducible and irreducible CFGs.** A **back edge** is an edge whose target
dominates its source; its target is a **loop header**. The natural loop of a back
edge is the header plus all nodes that reach the edge's source without passing
through the header. A CFG is **reducible** when every cycle has a single entry
that dominates the whole cycle — equivalently, in a depth-first traversal every
retreating edge is a back edge. Compilers usually see reducible CFGs from
structured source, but general CFG transformations and hand-written MIR can
produce irreducible ones.

**Relooper and stackifier.** Two established approaches recover structure.
Zakai's **Relooper** (Emscripten) handles arbitrary reducible or irreducible
graphs by introducing an explicit control variable and a dispatcher when a
graph has multiple entries into a cycle. Gohman's **stackifier** (the LLVM
WebAssembly backend) exploits reducibility: it walks the dominator tree and
maps loop headers and joins to `loop` and `block`, emitting depth-relative
branches directly. This design follows the stackifier for reducible input and
keeps a Relooper-style dispatcher as the fallback for irreducible input.

**Tail calls.** WebAssembly 3.0 standardizes tail calls: `return_call` for a
direct tail call and `return_call_ref` for a call through a typed function
reference. A tail call reuses the caller's frame, so deep tail recursion does
not grow the stack. A self tail call need not use the proposal at all: if the
callee is the caller, redirecting the back edge to the function entry with
updated parameters produces a loop, which is cheaper and works on any profile.
PureScript has no loop syntax — recursion is the only iteration
([functional core](../../frontend/semantics/functional-core.md)) — so loopification is what makes
idiomatic recursive loops cheap.

## Model

### MIR terminators

The terminator set is extended to:

```text
Terminator = Return      { value: ValueId, span }
           | Jump        { target: BlockId, arguments: [ValueId], span }
           | Branch      { condition: ValueId, then_block: BlockId, else_block: BlockId, span }
           | Switch      { value: ValueId, cases: [(i32, BlockId)], default: BlockId, span }
           | ReturnCall    { function: SymbolId, arguments: [ValueId], span }
           | ReturnCallRef { function: ValueId, arguments: [ValueId], span }
```

`Branch` drops its `merge_block` hint; the structurer discovers joins from the
graph. `Switch` dispatches on an `i32` tag or label; its case values are unique
`i32`s and `default` is always present — for a total match over a closed sum it
targets a trap block or the last arm. `ReturnCall` names a direct callee symbol
and `ReturnCallRef` a value of typed function-reference type. For a closure
tail call, P9 projects the code reference and passes the closure as the receiver
argument before forming `ReturnCallRef`; the terminator never takes a closure
object as its function operand.

### Structured Wasm model

The thin Wasm IR gains two structured nodes and keeps `If`:

```text
Op = Leaf(wasm_encoder::Instruction)
   | If   { then_body: Body, else_body: Body, result: Option(ValType), span }
   | Block { body: Body, result: Option(ValType), span }
   | Loop  { body: Body, result: Option(ValType), span }
```

Branch targets are not stored as absolute labels in the IR; the structurer emits
`br`/`br_table` as `Leaf` instructions whose immediates are depth-relative label
indices computed from the active-label stack. `Block` and `Loop` carry a result
type so a value-producing loop or join can leave its value on the stack; MIR
block parameters are still lowered as locals copied at each jump.

### Invariants

- Every block reachable from the entry has a terminator; the entry block exists.
- `Switch` case values are unique; every case and the default target exists;
  the selector is `i32`.
- A `ReturnCall`/`ReturnCallRef` leaves no live value in the calling function:
  its arguments are the only live values at the call, and the callee's result
  becomes the caller's result.
- Every emitted branch depth points at an enclosing `block` or `loop` label;
  a label-stack assertion rejects any miscomputed depth before encoding.
- Only genuinely irreducible input that the dispatcher cannot encode is
  rejected; reducible input never fails.
- Structuring preserves the value semantics of every existing MIR instruction
  and terminator; it changes only control shape.

## Design

### Drop `merge_block` and structure generally

`Branch { merge_block }` is a temporary hint: it lets a linear structurer emit
an `if`/`else`/`end` without discovering the join. It cannot express a loop and
over-constrains MIR. The `merge_block` field is removed. MIR carries no
structuring decision: the structurer derives every control shape from the CFG
edges. The nearest common descendant of a branch or switch (`common_join`) is
still computed for analyses and optimizations that need a join, but it is not
an emission precondition. A branch whose arms terminate independently has no
join and is still structured: each arm becomes its own region unit and the
branch targets the arms' labels directly.

### Reducible structuring

Every reducible CFG — with or without loops — is reduced to the same
`RegionPlan`: a dominator and natural-loop analysis partitions the reachable
blocks into nested regions, and each region is a topologically ordered list of
units, where a unit is a basic block or a nested natural loop. One emitter
(`RegionOps`) walks that plan for every reducible function, so loops and
acyclic diamonds, switches, shared successors, early returns, and traps all
share the same emission path. The dispatcher is the only other path and is
selected solely for irreducible input.

The plan is built and emitted as a stackifier over the dominator tree:

1. Compute reachability, predecessors, dominators, back edges, and natural
   loops, and check that the natural loops nest.
2. Partition each region's members into units: a natural loop becomes a `Loop`
   unit with its own nested region, and every remaining block becomes a `Block`
   unit. Topologically order the units after removing back edges, so each block
   is emitted exactly once and every forward edge targets a later unit's
   `Block` label.
3. Emit each unit inside nested `Block` regions: a `Loop` unit opens a Wasm
   `loop`, and later units are enclosed by `Block`s so a forward branch to a
   unit exits to its label. Emit each terminator as `br`/`br_if`/`br_table`
   with label depths resolved against the active-label stack, copying jump
   arguments into target locals.

This handles nested loops, multiple exits, and `Switch` uniformly.

### Irreducible structuring

When a cycle has more than one entry (an irreducible region), no static nesting
can encode it. The fallback is a **dispatcher**: one local `state` selects the
current basic block, an outer `loop` repeatedly dispatches with `br_table`, and
each block body assigns `state` before branching back to the dispatcher. This is
the Relooper fallback. It is only reached for input the reducible path cannot
handle, and it is validated by execution tests rather than assumed.

### Self-recursion to a loop

A self tail call is rewritten to a back edge to a new loop header instead of a
`return_call`. The original function entry remains a preheader: it passes the
function parameters to the loop header's block parameters. Each recursive call
evaluates all arguments before jumping to that header with the new values. The
header parameters replace uses of the original parameters in the loop body,
preserving SSA dominance even when arguments swap positions. The structurer
then sees an ordinary natural loop and encodes it as a `loop`, so
deep self-recursion runs in constant stack on every profile, including profiles
with the tail-call proposal disabled. Non-self tail calls use
`return_call`/`return_call_ref` and require the proposal.

### Tail calls and the capability profile

Tail calls are a target capability: the stable profile currently disables them
([capability profile](../wasm/capability-profile.md),
[DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md)). Enabling the
emission path requires revising that profile to expose `tail_call` and adding
execution evidence; the lowering must not emit a `return_call*` when the flag is
off, and must fall back to an ordinary call plus return.

### Rejected alternatives

- **Keep `merge_block` diamonds.** Rejected: they cannot encode loops, and they
  let MIR carry a structuring decision that belongs to P10.
- **Always emit a dispatcher state machine.** Rejected as the default: it adds a
  state local and an indirect dispatch to every function, obscuring reducible
  control and hurting engines that optimize structured loops. It is retained
  only as the irreducible fallback.
- **Structure before MIR.** Rejected: MIR is deliberately an arbitrary CFG
  because SSA/CFG is the natural substrate for verification and optimization;
  structure is recovered once, late, where it is needed.
- **Keep tail calls disabled and rely on the host stack.** Rejected: idiomatic
  PureScript tail recursion would grow the stack and abort.
- **Use `return_call` for self-recursion.** Rejected: a loop is cheaper and
  profile-independent; `return_call` is reserved for non-self tail calls.
- **Model branches as block results and use Wasm stack typing.** Rejected for
  now: MIR block parameters are already lowered as locals, so structuring stays
  independent of the multi-value work.

## Algorithms

### Dominators

The standard iterative fixed point is used (MIR's verifier already computes
dominators this way in `mir/verify/function.rs`); a production structurer may use
the Cooper–Harvey–Kennedy algorithm, which is near-linear and simpler to
implement over reverse postorder.

```text
dominators(entry):
    dom(entry) = { entry }
    for b != entry: dom(b) = all blocks
    repeat until fixed point:
        for b in reverse_postorder(entry):
            if preds(b) is empty: dom(b) = { b }
            else: dom(b) = { b } ∪ ⋂ { dom(p) | p in preds(b) }
    return dom
```

### Natural loops

```text
natural_loops(dom):
    for each edge u -> v with v ∈ dom(u):      // back edge
        header = v
        loop = { header } ∪ { u }
        work = [u]
        while work is non-empty:
            w = pop(work)
            for p in preds(w):
                if p ∉ loop:
                    loop.insert(p); work.push(p)
        add loop(header, body = loop)
```

A back edge whose target does not dominate its source indicates irreducible
control flow (a retreating edge that is not a back edge, or a header reached
from outside its own loop body); such regions are routed to the dispatcher.

### Reducible stackifier

```text
build_region(members, header, entry):
    partition members into units:
        each natural loop whose header != header -> Loop(header, build_region(loop.blocks, header, header))
        each remaining block                         -> Block(block)
    topologically order units after removing back edges (entry unit first)
    return RegionPlan { header, units, span }

emit_region(region, labels):
    // Emit units in order. Wrap the body accumulated so far in a Block for the
    // current unit so a forward branch to a later unit exits to its label.
    for (i, unit) in region.units:
        sequence = [Block(body = sequence, span = unit.span)]
        active = labels + [Loop(region.header)] + [Block(u.entry) for u in region.units[i+1..].rev()]
        emit_unit(unit, active, sequence)

emit_unit(unit, labels, body):
    match unit:
        Block(b)            -> emit_body(b, labels, body)
        Loop{header, body=r} -> emit_region(r, labels, loop_body); body.push(Loop(loop_body))

emit_body(node, labels, body):
    emit instructions of node
    match node.terminator:
        Return v            -> emit load v; return
        Jump(t, args)       -> read all args; write t's locals; emit br(depth(t, labels))
        Branch(c, t, f)     -> emit load c; br_if(depth(t, labels)); br(depth(f, labels))
        Switch(v, cases, d) -> emit load v; br_table([depth(b,labels) for b in cases],
                                                     depth(d, labels))
        ReturnCall(f, args)    -> emit return_call f(args)        if profile allows
        ReturnCallRef(f, args) -> emit return_call_ref f(args)    if profile allows
```

Every basic block is a unit and appears exactly once, so no block is duplicated
and no join is required for emission: two arms that both return become sibling
units and the branch targets their labels. A `Loop` unit's own region repeats
the partition for the loop body. `depth(label, labels)` is the number of active
labels strictly inside `label`, i.e. the distance from the top of the stack to
the matching entry. The stack invariant is checked before each emission: a
target not present in `labels` is a structuring bug and is rejected with a
source-associated diagnostic. Jump arguments are all read before any target
local is written, so a jump that swaps two block parameters stays correct.

### Irreducible dispatcher

```text
emit_dispatcher(blocks, entry):
    state = fresh i32 local
    emit state = index(entry)
    emit loop $dispatch:
        emit block $b_0 ... block $b_{n-1}:        // one block per block
            emit br_table(state) -> [b_0, ..., b_{n-1}] default trap
        // Each block body:
        for b in block order:
            emit body of b
            match terminator:
                Branch(c, t, f) -> emit if c then state = index(t) else state = index(f)
                Jump(t, _)      -> emit state = index(t)
                Return v        -> emit return v
            emit br $dispatch
        close all blocks and the loop
```

### Tail-call classification

A call is in tail position when its result is returned directly and no later
use observes it. The analysis runs on MIR blocks after P9 has built the CFG:

```text
mark_tail(function):
    if a self tail call exists:
        create a loop header with block parameters matching function parameters
        make the original entry a preheader that jumps to the header
        rewrite body uses of function parameters to the header parameters
    for each reachable block b:
        for each final instruction a in b:
            d = a.destination
            if a is Call/ClosureCall and b's terminator is Return(d)
               and d has no other use:
                if callee == function.symbol:
                    rewrite to a Jump(loop_header, a.arguments) // header has block params
                else if is_function_ref_call(a):
                    replace with ReturnCallRef(a.function, a.arguments)
                else:
                    replace with ReturnCall(a.function, a.arguments)
```

Both arms of a lowered Core `If` can qualify independently when each ends in
a return. A tail call's destination must have no other users; the MIR
verifier confirms this after the rewrite.

### Edge cases

- **Unreachable blocks.** Blocks unreachable from the entry are removed before
  structuring; the verifier treats the entry-reachable subgraph as the function.
- **Loop with multiple exits.** Each exit is a `Block` label; every branch to it
  gets the corresponding depth.
- **Branch to the current loop header.** Encoded as `br` to the `loop` label
  (continue), not a `block` (break).
- **Nested loops and joins.** The active-label stack handles nesting; joins
  nested inside loops open their own `block`.
- **`Switch` default.** Always present; an exhaustive closed sum uses a trap
  block or the last case, and a default arm uses its branch.
- **Drop-through.** A block with one successor and no other predecessor can be
  inlined into its predecessor; the stackifier emits its body and continues
  rather than branching.
- **Irreducible region with an entry block.** The entry is dispatched first;
  if it is also a cycle entry, the dispatcher covers it like any other block.
- **`return_call*` at a disabled capability.** Lowered as an ordinary call
  followed by a return, growing the stack but remaining correct.

## Code map

Control flow has three owners: MIR, which defines the terminators; `mir/lower/`,
which produces them (including tail-position analysis and self-recursion
loopification); and the Wasm structurer, which recovers structured
`block`/`loop`/`if`/`br`/`br_table`.

```text
mir/
  mod.rs              # Terminator: Return, Jump, Branch, Switch,
                      # ReturnCall, ReturnCallRef (no merge_block)
  instruction.rs      # Instruction vocabulary inspected by tail analysis
  lower/
    assignments.rs    # lower CC If/Case to Branch and tag-based Switch
    tail.rs           # tail-position analysis, self-loopification, ReturnCall*
  verify/function.rs  # dominance, terminator, and liveness checks
wasm/
  mod.rs              # Op::Leaf, Op::If, Op::Block, Op::Loop
  lower/
    structure.rs      # structure_module entry, label stack, emission
    structure/
      cfg.rs          # dominators, natural loops, and the reducible region plan
      region.rs       # emission for every reducible region plan
      dispatcher.rs   # Relooper-style fallback for irreducible regions
      instructions.rs # leaf emission and jump-argument copies
      ops.rs          # br/br_table/return_call* opcode emission
  encode.rs           # binary encoding of structured bodies
capability.rs         # tail_call and related proposal flags
```

Required types and entry points:

- `mir/mod.rs` must define the terminator set of this document. `Branch` must
  not carry a `merge_block`; `Switch` must carry an `i32` selector, unique case
  values, and an always-present default. `ReturnCall` must name a `SymbolId`
  and `ReturnCallRef` a typed function-reference value, each with argument
  values and a span. `mir/instruction.rs` must hold the `Instruction` forms
  that tail-position analysis and lowerers inspect (`Call`, `CallRef`, and the
  value-producing instructions).
- `mir/lower/tail.rs` must expose the tail-call rewriting entry point:

  ```rust
  fn mark_tail(function: &mut MirFunction) -> Result<(), MirError>;
  ```

  It must rewrite a self tail call to a jump to a loop header with block
  parameters, reached initially from an entry preheader. A non-self call
  through a typed function reference becomes `ReturnCallRef`; another non-self
  tail call becomes `ReturnCall`. When the profile disables `tail_call`, it
  emits an ordinary call plus return instead.
- `mir/lower/assignments.rs` must lower CC `If` and tag-based `case` to
  `Branch` and `Switch` without a structuring hint; joins are derived later.
- `wasm/lower/structure.rs` must expose the structuring entry point:

  ```rust
  fn structure_module(
      module: &MirModule,
      layout: &PlannedLayout,
      target: TargetCapabilities,
  ) -> Result<WasmModule, StructureError>;
  ```

  It must maintain the label stack and reject any `br`/`br_table` whose target
  is not an enclosing `Block`/`Loop` label.
- `wasm/lower/structure/region.rs` must implement one emitter for every
  reducible region plan, opening `Op::Loop` at a natural-loop header and
  `Op::Block` for each region unit, and targeting them with depth-relative
  branches; it must not consult `common_join` to decide whether a branch or
  switch can be emitted. `wasm/lower/structure/dispatcher.rs` must implement
  the irreducible fallback, and the entry point must select it only for
  irreducible input. `wasm/mod.rs` must provide `Op::Block` and `Op::Loop`
  alongside `Op::Leaf` and `Op::If`, each carrying an optional result type and a
  span.
- `wasm/encode.rs` must encode structured bodies and depth-relative branches,
  and `mir/verify/function.rs` must verify dominance, the `Switch` shape, and
  that a tail call's destination has no other users.
- `capability.rs` must expose the `tail_call` flag that gates `return_call*`.

The label-stack invariant is the structurer's contract: every emitted branch
depth must resolve to an active enclosing label. The only admitted structuring
failure is a genuinely irreducible region for which the dispatcher was not
selected; any other unresolved depth is a compiler bug reported with the
terminator's span. The `Switch` contract with pattern matching is owned by
[pattern matching](pattern-matching.md), and the module skeleton and section
encoding by
[Wasm encoding](../wasm/encoding-and-structuring.md).

## Invariants and verification

- **MIR verifier.** `Terminator::Switch` checks the selector type, unique case
  values, and existing targets. `Terminator::Branch` no longer reads a
  `merge_block`. `ReturnCall`/`ReturnCallRef` check the callee signature and
  that the arguments are the only live values. Dominance is rechecked on the
  rewritten CFG.
- **Structurer.** Every emitted `br`/`br_table` depth is checked against the
  active-label stack. The only rejected input is a genuinely irreducible region
  for which the dispatcher was not selected. A miscomputed depth is a compiler
  bug, reported with the terminator's span.
- **Wasm validation.** The encoded module is validated independently by
  `wasmparser` with the selected capability profile; the validator rejects a
  malformed label or block type before execution.
- **Execution evidence.** Loops, multi-way and nested matches, and tail calls
  require `wasmtime` execution tests. A deep tail-recursive fixture proves no
  stack growth; an iterative loop fixture proves constant stack for
  self-recursion. Evidence is tracked in
  [data representation](data-representation.md) and `BE-13`/`BC-02`/`BC-08` in
  [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md).

## Worked example

A tail-recursive sum:

```purescript
sum :: Int -> Int -> Int
sum n acc = if n == 0 then acc else sum (n - 1) (acc + n)
```

CC lowering emits an `If` whose then-arm is `acc` and whose else-arm computes
`n' = n - 1`, `acc' = acc + n` and calls `sum`. Before loopification the MIR is:

```text
B0: v = Primitive IntEq(n, 0)
    Branch(v, B1, B2)
B1: Return acc
B2: n' = Primitive IntSub(n, 1)
    acc' = Primitive IntAdd(acc, n)
    ReturnCall sum [n', acc']
```

The classification sees that the call's destination is the function result and
that the callee is `sum` itself, so it rewrites `ReturnCall` into a jump to the
entry with the same parameters. The structurer sees one natural loop and emits:

```text
loop $head
    v = i32.eq n 0
    br_if $exit, v
    n   = i32.sub n 1
    acc = i32.add acc n
    br $head
end
return acc
```

`br_if $exit` targets the enclosing `block` label (distance 1); `br $head`
targets the `loop` label (distance 0). No frame is created per iteration, so
`sum n 0` for large `n` runs in constant stack even on a profile without tail
calls. A non-self tail call such as `f x = g x` lowers to `return_call g(x)`,
which requires the tail-call capability.

## Boundaries and interfaces

- **From pattern matching.** A tag decision becomes a `Switch` terminator; nested
  field tests become projections plus `If` diamonds
  ([pattern matching](pattern-matching.md)).
- **From MIR lowering.** `Branch` no longer carries a merge block; the structurer
  derives every control shape from the CFG edges, and a branch or switch whose
  arms have no join is still emitted. CC's expression-level `If` and `case`
  still lower to diamonds, but the structuring hint is gone.
- **To encoding.** `Block`, `Loop`, and depth-resolved `br`/`br_table` are
  emitted as structured `Op`s; leaf opcodes remain
  `wasm_encoder::Instruction` under the thin-encoding rule of
  [DEC-02](../../../decision/DEC-02-thin-structured-wasm-encoding.md).
- **Capability profile.** Emitting `return_call*` depends on the `tail_call`
  flag; a profile revision is part of the design, not a silent fallback.
- **Not owned.** Value layout and calling conventions ([MIR](mir.md)), pattern
  coverage ([pattern matching](pattern-matching.md)), and the module skeleton
  ([Wasm encoding](../wasm/encoding-and-structuring.md)).

## Open questions and future work

- **Unreachable-block elimination.** The design assumes unreachable blocks are
  removed before structuring; a MIR-level pass must define and test this.
- **Multi-value regions.** `Block`/`Loop`/`If` carry at most one result today;
  block parameters as Wasm block results await the multi-value work (`BC-05`).
- **Branch hints.** WebAssembly branch hinting is a standard feature; a hint
  from the source or a profile could improve loop codegen but is not required.
- **Exceptions and stack switching.** Structured exception regions and stack
  switching are separate proposal families (`BC-08`, `BC-09`); each would add
  its own structured node and capability flag.
- **Irreducible production inputs.** No current frontend lowering produces
  irreducible control flow. A direct-MIR execution fixture validates the
  dispatcher semantics; code-size and runtime costs remain to be measured.
- **Optimization interaction.** Loop rotation, unrolling, and tail-call
  inlining belong to [MIR optimization](../opt/mir.md) and must preserve the
  structuring invariants.

## Implementation notes

CC now preserves a constructor-only, unique-tag case as `TagSwitch`, which P9
lowers to a `Switch` with parameter-free successor blocks. The MIR verifier
checks the `i32` selector, unique tags, and successor shape. P10 analyzes the
reachable CFG for every function, computes reachability, predecessors,
dominators, back edges, and natural loops, checks that loop regions nest, and
partitions each region into an ordered list of block and loop units. The same
emitter handles every reducible function, with or without loops: it emits
`Loop` at each natural-loop header, encloses each unit in a `Block` for forward
targets, and resolves `Jump`, `Branch`, and `Switch` depths against the active
label stack. A branch or switch whose arms terminate independently has no
`common_join` and is still emitted, so a valid CFG is never rejected for lack
of a join. Sparse and reordered signed switch tags are mapped to dense unsigned
indices before `br_table`. Non-parameter reference locals are declared nullable
and restored with `ref.as_non_null` at each read, because Wasm forbids reading a
non-defaultable local that was initialized inside an inner structured block.
Execution coverage includes a loop with loop-carried values, nested loops with
multiple exits, a diamond and a switch inside a loop, an early-return and a
trapping arm, and a shared successor with swapped block arguments; generated
modules pass the Wasm IR verifier and `wasmparser` validation.

The implementation remains narrower than the complete design in these
specific areas:

- The `merge_block` hint has been removed. `Branch` carries only its condition
  and two targets, and the nearest common join is derived from the CFG edges.
  The derivation lives in `mir/cfg.rs` (`common_join`, `join_blocks`) and is
  used by analyses and optimizations such as the constant-parameter optimizer.
  It is not an emission precondition: the structurer emits every reducible
  block once and targets it by label, so no branch or switch needs a join.
- Every reducible CFG, including nested loops and multiple loop exits, is
  structured directly by the region emitter. Only a reachable cyclic SCC with
  multiple entry blocks uses a function-level dispatcher with an `i32` state
  local, nested dispatch blocks, and `br_table`. Jump arguments are copied
  before state updates; branches and switches select the next state, including
  sparse signed switch tags. A direct-MIR Wasmtime fixture exercises
  parameterized jumps, branches, and switch cases plus the default path.
- Duplicate constructor alternatives retain source-order first-match behavior
  by using the existing chain of `If` decisions instead of `TagSwitch`.
- `TargetCapabilities::tail_call` controls Wasm validation features, but no MIR
  tail-call terminator or `return_call*` emission exists yet. Tail-call marking
  and self-recursion loopification are still unimplemented, so deep
  self-recursion is not yet loopified and reference/direct tail calls are not
  rewritten; see the acceptance record for the remaining work. The stable
  profile keeps the flag disabled.

## References

- Zakai, A., *Relooper* (Emscripten), the multi-entry loop restructuring
  algorithm.
- Gohman, D., *WebAssembly control flow* / stackification in the LLVM
  WebAssembly backend.
- Cooper, K., Harvey, T., and Kennedy, K., *A Simple, Fast Dominance Algorithm*
  (2001).
- WebAssembly 3.0: structured control flow and the tail-call proposal
  (`return_call`, `return_call_ref`).
- [DEC-02](../../../decision/DEC-02-thin-structured-wasm-encoding.md): thin
  structured Wasm encoding.
- [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md) and
  [capability profile](../wasm/capability-profile.md): target features and the
  tail-call gate.
- [MIR](mir.md) and [IR boundaries](../00-ir-boundaries.md): the CFG contract.
