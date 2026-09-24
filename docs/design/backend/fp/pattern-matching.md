# Pattern Matching

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md), [CC IR](cc-ir.md), and
the constructor model of [data representation](data-representation.md);
algebraic data types, first-match semantics, and basic decision procedures. Read
[IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** Source patterns are compiled to a decision DAG by the classical
matrix algorithm (specialize/default, with sharing), and the DAG is realized by
ordinary CC variant tests, projections, and conditionals. The same matrix
computation decides exhaustiveness and redundancy and produces a witness
pattern; one `case` evaluates each scrutinee exactly once and preserves
top-to-bottom, left-to-right first-match order.

## Scope

This document owns the pattern decision boundary: the pattern matrix, the
decision DAG, exhaustiveness and redundancy analysis with witnesses, and the CC
lowering that realizes a selected alternative. It does not own pattern type
checking or the source syntax of patterns (frontend; see
[frontend type inference](../../frontend/type-system/type-inference.md) and `FE-06`/`FE-12` in
[DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md)), the concrete
constructor and record layout ([data representation](data-representation.md),
[CC IR](cc-ir.md)), or
multi-way control-flow structuring and tail calls
([control flow and tail calls](control-flow-and-tail-calls.md)).

## Background

A **pattern** destructures a value by testing its outer constructor and binding
or recursively destructuring its fields. PureScript gives patterns
**first-match** semantics: alternatives are tried top to bottom, and within a
constructor pattern fields are matched left to right. Literals and guards break
the pure constructor algebra because their tests are not constructor
decompositions; they are handled as **refutable tests** with a fall-through edge
to the next alternative.

The reference accounts are Augustsson, *Compiling Pattern Matching* (1985),
which introduced the **pattern matrix** as a uniform representation of a set of
alternatives; Wadler, *Efficient Compilation of Pattern Matching* (1987), which
popularized **specialize** and **default**; and Maranget, *Warnings for Pattern
Matching* (2007) and *Compiling Pattern Matching to Good Decision Trees* (2008),
which give the usefulness algorithm used here for both compilation and
diagnostics. A decision **tree** tests a constructor once per leaf path; a
decision **DAG** shares equal subproblems so a constructor is tested once even
when several rows need it. Sharing does not change first-match semantics because
the test result is a function of the scrutinee, not of the row being tried.

Two conventions matter for correctness. A constructor is identified by its
**case tag** within one sum type, not by a runtime type tag; the erased
representation already recovers polymorphism ([polymorphism and
erasure](polymorphism-and-erasure.md)). A record is a one-case product: its
outer test is trivially true and it specializes by projection.

## Model

### Patterns and scrutinees

Typed Core carries source-oriented patterns so diagnostics keep their spans
(`psrs_core::Pattern`, `psrs_core::PatternKind`, `psrs_core::CaseBranch`):

```text
Pattern = Wildcard
        | Var { id, ty }
        | Constructor { symbol: SymbolId, arguments: [Pattern] }
        | Record { fields: [(label, Pattern)] }
```

Wildcards and variables are **irrefutable**: they match every value.
Constructors and records are **refutable** when they have more than one sibling
case (constructors) or are always-irrefutable productions (records, newtypes,
and single-constructor data types). A **scrutinee** is a value already bound in
CC, and each `case` has one scrutinee.

### Pattern matrix

A `case` is compiled as a matrix whose rows are the alternatives and whose
columns are the scrutinee positions:

```text
Matrix  = [Row]
Row     = { patterns: [Pattern], branch: usize, span: TextRange }
Surface = { type: TypeId, cases: [CaseSignature] }
CaseSignature = { tag: i32, arity: usize, irrefutable: bool }
```

`branch` is the index into the original `Core` `branches`; it is the value the
decision carries to the diagnostic and to the branch body. `CaseSignature.tag`
is the stable constructor tag from the representation table; `irrefutable` is
true for wildcards, variables, records, and single-constructor data types.

### Decision DAG

The compiled form is a DAG of decisions over columns, with constructor tests,
literal tests, projections, and variable bindings.

```text
Decision = Leaf   { branch: usize }
         | Fail
         | Switch { column: usize,
                    edges:  [(Test, Decision)],
                    default: Option<Decision> }
Test     = Constructor { type: TypeId, tag: i32 }
         | Literal(Literal)
         | Irrefutable

Action   = Project { field: u32 }        // from a bound scrutinee
         | Bind    { id: LocalId }        // bind a value to a pattern variable
         | TestTag { type: TypeId, tag: i32 }
```

`Switch.edges` are the constructor or literal tests with their sub-decisions.
`Switch.default` is taken when no edge matches; it is `None` only when the type
is fully covered by `edges`, in which case a mismatch is a compiler bug and is
lowered to a trap. Specialization threads `Action`s along each edge so the
lowering emits projections and bindings exactly where the pattern introduces
them.

### Invariants

- Alternatives keep source order; the first matching branch wins.
- Every scrutinee expression is evaluated exactly once, before any test.
- Nested tests project from an already-bound value; no source expression is
  re-evaluated.
- A `Switch` on a sum type has at most one edge per case tag, and each tag is
  either an edge or in `default` (never both).
- Every `Leaf.branch` indexes the original `branches` and keeps its span.
- An alternative after an irrefutable one is unreachable and reported as
  redundant.
- Coverage is computed over the same matrix; a non-exhaustive match yields a
  witness pattern, and a redundant row yields a witness value it can never match.

## Design

### Chosen compilation

Compile each `case` by the matrix algorithm of Maranget (2008):

1. **Column selection.** Pick the column whose patterns expose the fewest
   distinct constructors (a heuristic that keeps the DAG small). A column with
   only irrefutable patterns is not tested at all.
2. **Specialize.** For a constructor `c` in the selected column, keep the rows
   whose pattern is `c` or irrefutable, replace the column by `c`'s subpatterns,
   and recurse on the smaller matrix. Irrefutable rows (wildcards or variables)
   are expanded to `c`'s arity by repeating the irrefutable pattern, so the
   recursion has a fixed column count.
3. **Default.** For rows whose selected pattern is irrefutable, remove the
   column and recurse. The default branch is taken only after all constructor
   edges fail.
4. **Share.** Hash-cons the recursive subproblems keyed by their normalized
   matrix; equal subproblems become one `Decision` node. This turns the tree
   into a DAG and tests a constructor once.

Records and single-constructor data types specialize immediately with a
projection and no tag test. Nullary sum types specialize to a `Switch` whose
edges carry no projections. Newtypes erase to their field; a newtype pattern is
compiled as the field pattern against the scrutinee itself
(`cc/case/mod.rs::lower_newtype_case`).

### Coverage and diagnostics

The same algorithm answers usefulness questions:

- A row `r` is **redundant** when the rows above it already match every value
  `r` could match, i.e. when `r` is not *useful* with respect to them.
- The match is **exhaustive** when the matrix's default part necessarily has a
  row for every value of the scrutinee type; otherwise the *uncovered* set is
  non-empty and yields a witness pattern such as `Some _` or `Red`.

Witness construction is a byproduct of the usefulness recursion: when a
subproblem has no covering row, the algorithm returns a pattern that the
existing rows cannot match. Non-exhaustive matches and redundant rows are
frontend diagnostics; the compiler never silently emits a partial decision.
Coverage is checked where the pattern matrix is built, so a rejected program
keeps its source span.

### Rejected alternatives

- **Ordered backtracking matcher (nested if-chain).** The current lowering
  produces nested if-diamonds with `IntEq` comparisons. Rejected as the target:
  it re-tests constructors for every row, duplicates row bodies on fall-through,
  cannot share nested work, and makes exhaustiveness and redundancy checks
  separate ad-hoc analyses. It also risks re-evaluating scrutinees.
- **Decision tree without sharing.** Correct but duplicates shared subproblems
  across rows, growing code exponentially on nested matches.
- **Compile straight to MIR/CFG.** Rejected: the decision is target-neutral and
  must not name Wasm types, casts, or basic blocks. Representation tests and
  control flow belong to CC lowering and MIR structuring respectively.
- **Flatten each sum to its tag and linearly search constructor arms.**
  Rejected: it does not compose with nested patterns and leaves the default
  matrix implicit, so coverage cannot be proven from the compiled form.
- **Dynamic type tags to distinguish constructors of different sum types.**
  Rejected: constructor identity is the case tag within one sum; source type
  arguments are erased ([DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md)).

## Algorithms

### Decision compilation

```text
compile(matrix, columns):
    if matrix is empty:
        return Fail
    if the first row has only irrefutable patterns:
        return Leaf { branch = first row.branch }
    column = choose_column(matrix, columns)
    if column has no constructor or literal patterns:
        // Only irrefutable heads remain; drop the column and recurse.
        return compile(default(matrix, column), columns - column)

    // Collect the refutable heads appearing in the chosen column.
    tests = distinct_heads(matrix, column)          // by tag or literal
    edges = []
    for test in tests:
        edges.push((test, compile(specialize(matrix, column, test), columns)))
    // A closed constructor signature proves whether the remaining tags exist.
    default_decision = if column_has_irrefutable_rows(matrix, column)
                          or tests do not cover the closed signature:
        Some(compile(default(matrix, column), columns - column))
    else:
        None
    return share(Switch {
        column,
        edges,
        default = default_decision,
    })
```

The base case checks the **first** row: a later wildcard may not preempt an
earlier constructor row. A missing default compiles to `Fail` and must be
unreachable only when coverage has proved the tested signature complete.

`specialize(matrix, column, Constructor c)` keeps rows whose `column` pattern is
`c` or irrefutable; for each kept row it removes the column and splices in `c`'s
subpatterns (for an irrefutable row it splices `arity(c)` wildcards).
`default(matrix, column)` keeps only rows whose `column` pattern is irrefutable
and removes the column. `share` interns the normalized subproblem in a table;
two structurally equal recursions under the same column return the same node.

### Usefulness, exhaustiveness, and redundancy

Usefulness `U(matrix, q)` decides whether the query row `q` matches some value
not matched by `matrix`. Exhaustiveness is `not U(matrix, [wildcards])`;
redundancy of row `i` is `not U(matrix[0..i], matrix[i])`.

```text
useful(matrix, query):
    if query is empty:
        return matrix is empty

    head = query[0]
    match head:
        Constructor c:
            return useful(specialize(matrix, 0, c),
                          c_subpatterns(query))
        Literal l:
            return useful(specialize(matrix, 0, l), query[1..])
        Irrefutable:
            if matrix has a full first-column signature S:
                // "Default" must cover every constructor c in S.
                for c in S:
                    if useful(specialize(matrix, 0, c),
                              wildcards(arity(c)) ++ query[1..]):
                        return true
                return false
            else:
                return useful(default(matrix, 0), query[1..])
```

A witness is produced by returning a concrete pattern at the `Irrefutable`
leaves: for a constructor case, prepend the constructor to the sub-witness; for
the default case, pick any missing constructor and fill its fields with
wildcards. Redundancy reports the row's own pattern; a non-exhaustive match
reports the witness with the scrutinee type's constructors.

`matrix has a full first-column signature` means every constructor of the
scrutinee type occurs in the first column or can be supplied by an irrefutable
row. This is what makes `Some _` (only) non-exhaustive while `_ | Some x`
exhaustive.

### Realizing a decision in CC

The lowering walks the DAG with an environment mapping each column to its
current CC `ValueId`:

```text
lower(decision, env):
    match decision:
        Leaf { branch } -> lower_branch(branch, env)
        Fail            -> trap
        Switch { column, edges, default }:
            value = env[column]
            fallback = lower(default, env) if default exists else trap
            for (test, sub) in reverse(edges):
                cond = test_constructor_tag_or_literal(test, value)
                selected = lower_projecting(sub, value, test)
                fallback = If(cond, selected, fallback)
            return fallback
```

`lower_projecting` emits one `VariantGet` or `ProductGet` per field of the
specialized constructor (skipping wildcard fields), binds variables, and
recurses. A `Switch` whose type is a nullary sum has an empty test column, so it
compiles to a chain of `If`s over integer tags; when tags are dense and the
outer value is already the tag, the target is a MIR `Switch` rather than a chain
([control flow and tail calls](control-flow-and-tail-calls.md)).

### Edge cases

- **Empty `case`** (no alternatives): a frontend error; there is nothing to
  select and the result has no branch.
- **Only irrefutable alternatives:** the first is compiled with no test and the
  rest are reported redundant.
- **Single-constructor data type and records:** specialize immediately and emit
  projections; no tag test.
- **Newtypes:** erase; recurse on the field pattern against the scrutinee.
- **Mixed sums (some cases nullary, some with fields):** nullary edges carry no
  projection; field edges cast to the case type and project, per
  [CC IR](cc-ir.md).
- **Nested patterns:** each nesting level adds a column and an edge; projection
  happens once per field on the path.
- **Erased (parameter-dependent) fields:** project the erased value and unbox it
  at the point of use (`cc/case/erased.rs`), never re-running the outer test.
- **Guards and view patterns (planned):** a failed test falls through to the
  next alternative at the same matrix position; the matrix structure is
  unchanged and the guard is an extra refutable `Literal`-style edge.
- **Literal patterns (planned):** specialize on the literal value; the final
  default edge handles the infinite complement, so exhaustiveness over an
  infinite type requires an irrefutable alternative.

## Code map

The pattern stage has three owners: the decision compiler in `cc/case/`, which
builds and analyzes the decision DAG; `cc/lower/`, which realizes a decision as
CC assignments; and the MIR/Wasm side, which turns a tag dispatch into a
`Switch` terminator and then a `br_table`. Coverage and redundancy diagnostics
are produced by the decision compiler, not by the lowering.

```text
cc/
  case/
    mod.rs            # lower_case entry, newtype erasure, dispatch by pattern kind
    decision.rs       # matrix construction, compile, specialize/default, sharing
    coverage.rs       # usefulness, exhaustiveness, redundancy, witnesses
    aggregate.rs      # constructor patterns and nested columns
    record.rs         # record and single-constructor product patterns
    erased.rs         # parameter-dependent field recovery
  lower/
    case.rs           # realize a Decision as CC assignments
  layout/mod.rs       # constructor tags and variant representation requirements
mir/
  mod.rs              # Terminator::Switch produced from a tag decision
  instruction.rs      # instruction vocabulary the case lowerer emits
wasm/lower/structure/
  region.rs           # emit a Switch as a br_table inside structured control
```

Required types and entry points:

- The decision compiler must define the pattern model of this document:
  `Matrix`, `Row`, `Surface`, `CaseSignature`, the decision node `Decision`
  (`Leaf`, `Fail`, `Switch`), the matcher kinds `Test` (`Constructor`,
  `Literal`, `Irrefutable`), and `Action` (`Project`, `Bind`, `TestTag`).
- Its entry point must compile one `case` against one scrutinee:

  ```rust
  impl DecisionCompiler {
      fn compile_case(
          &mut self,
          scrutinee: ValueId,
          branches: &[CaseBranch],
      ) -> Result<Decision, CaseError>;
  }
  ```

- `cc/case/coverage.rs` must provide `useful(matrix, query) -> bool`, the
  exhaustiveness and redundancy checks, and a `Witness`. It must report a
  non-exhaustive match or a redundant row as a frontend diagnostic that
  preserves the scrutinee or alternative span.
- `cc/lower/case.rs` must realize a decision into CC assignments exactly once
  per scrutinee, in first-match order:

  ```rust
  fn realize_decision(
      decision: &Decision,
      branches: &[CaseBranch],
      cx: &mut CcLowerer,
  ) -> Result<ValueId, CcError>;
  ```

  It must project with `VariantGet`/`ProductGet` and must never re-test an outer
  constructor.
- `cc/case/mod.rs` must remain the single dispatch point for `ExprKind::Case`
  and must own newtype erasure and the choice between the aggregate, record, and
  erased sub-lowerings.
- `cc/layout/mod.rs` must supply the stable constructor tag per case used by
  `CaseSignature`; no decision node may name a Wasm type, cast, or basic block.
- The MIR side must define `Terminator::Switch` in `mir/mod.rs` as an `i32`
  selector with unique case values and a mandatory default, and
  `mir/instruction.rs` must carry the instruction forms the case lowerer emits.
- `wasm/lower/structure/` must emit a `Switch` as a `br_table` with
  depth-resolved labels; nested field tests remain `If` diamonds. The
  controlling contract is owned by
  [control flow and tail calls](control-flow-and-tail-calls.md), the
  constructor layout by [data representation](data-representation.md), and the
  erased-field protocol by
  [polymorphism and erasure](polymorphism-and-erasure.md).

Coverage is checked before lowering, so a generated decision must be total on
the scrutinee type; a missing default for a total `Switch` is a compiler bug.
The decision DAG is short-lived: `cc/lower/case.rs` must discard it once the CC
assignments are built.

## Invariants and verification

- The decision DAG is an internal, short-lived object; it is discarded after
  the CC assignments are built and is not a long-lived IR.
- CC verification checks every generated operation: tag and field selectors are
  in range for the representation, projections use the case type, and both
  `If` arms agree with the declared result shape (`cc/verify/variant.rs`,
  `cc/verify/ops.rs`).
- MIR verification checks that a `Switch` selector is an `i32`, its case values
  are unique, every case and the default target exists, and every block is
  dominated by the value it tests.
- Coverage and redundancy are checked before lowering, so a generated decision
  is total on the scrutinee type; a missing default for a total `Switch` is a
  compiler bug, not a source error.
- Branch indices and spans are preserved from the original `CaseBranch`; a
  diagnostic can always point back at the source alternative.

## Worked example

Match a small sum with one field:

```purescript
data Option = None | Some Int

toInt :: Option -> Int
toInt o = case o of
  Some n -> n
  None -> 0
```

The matrix has two rows over one column `o`:

```text
row 0: [Some n]  (branch 0)
row 1: [None]    (branch 1)
```

`Some` and `None` are the only constructors, so the first column has a full
signature and no default row. Specializing on `Some` gives the submatrix
`[[n]]`; specializing on `None` gives `[[]]`. Sharing is trivial here. The
target decision is:

```text
Switch(column 0, Variant[Option],
    edges: [
        Some -> Project field 0; Bind n; Leaf(0),
        None -> Leaf(1),
    ],
    default: Fail)
```

CC lowering emits a tag read and a branch per edge (or a single MIR `Switch` on
the tag):

```text
actual   = VariantTag $option(o)
expected = Constant 0                       // Some
cond     = Primitive IntEq actual expected
result   = If cond
    (then: n = VariantGet $option case=Some field=0 o; value = n)
    (else: value = Constant 0)
```

The verifier proves `o` is available at every use, the projection names the
`Some` case, and both arms of the `If` yield `Integer`. The `Some` edge is the
only one that projects, and it does so once.

## Boundaries and interfaces

- **From Typed Core:** a `case` with a single scrutinee and source-spanned
  patterns. Nested constructors, records, variables, and wildcards are
  translated; CC representation IDs or Wasm layouts must not cross this
  boundary.
- **To CC lowering:** a decision DAG plus the original branch bodies. CC owns
  representation tests and casts, projections, bindings, conditionals, and
  switches, and must not reorder alternatives.
- **To MIR/control flow:** a tag test targets `Terminator::Switch`; record and
  field tests become projections plus `If` diamonds
  ([control flow and tail calls](control-flow-and-tail-calls.md)).
- **To diagnostics:** exhaustiveness and redundancy witnesses carry the source
  span of the scrutinee or the offending alternative.
- **Not owned:** pattern syntax and type checking (frontend), constructor
  layout ([data representation](data-representation.md)), and structuring of the
  generated control flow ([control flow](control-flow-and-tail-calls.md)).

## Open questions and future work

- **Guards and view patterns.** The fall-through edge is specified, but the
  frontend does not carry guards yet; a guard that can call arbitrary code
  complicates the "evaluate each scrutinee once" invariant only if a view
  pattern introduces a new scrutinee, which is treated as an extra column.
- **Literal, tuple, array, and as-patterns.** Literals use the same specialize
  step with an infinite complement; tuples are products; arrays and as-patterns
  need either a linear prefix decomposition or a lowered helper call.
- **Or-patterns.** They require row duplication and affect redundancy
  reporting; the matrix algorithm supports them if the frontend normalizes them
  into separate rows that share a body.
- **Warning codes.** Redundancy and non-exhaustiveness need official
  `errorCode`/warning-code agreement (`M8-W` in
  [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md)).
- **Column-selection heuristics.** The current design specifies a
  fewest-constructors heuristic; cost models tuned to Wasm local count and
  branch depth remain open.

## Implementation notes

The current code does not implement the matrix algorithm. `cc/case/decision.rs`
records the reachable branch order and a single trailing wildcard/variable
fallback, and `cc/case/aggregate.rs` lowers the remaining alternatives as nested
`If` diamonds comparing `VariantTag` against a constant. Coverage is limited to
the diagnostic "non-exhaustive case requires a wildcard alternative"; there is
no redundancy analysis and no witness. `Terminator::Switch` and multi-way
`br_table` do not exist yet. Nothing in this document depends on those
temporary shapes.

## References

- Augustsson, L., *Compiling Pattern Matching* (1985).
- Wadler, P., *Efficient Compilation of Pattern Matching* (1987), in Peyton
  Jones, *The Implementation of Functional Programming Languages*.
- Maranget, L., *Warnings for Pattern Matching*, Journal of Functional
  Programming (2007).
- Maranget, L., *Compiling Pattern Matching to Good Decision Trees* (2008).
- [DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md)
  and
  [CC IR](cc-ir.md):
  erased parameterized fields and the target-neutral variant representation.
- [Control flow and tail calls](control-flow-and-tail-calls.md): `Switch` and
  structured control.
