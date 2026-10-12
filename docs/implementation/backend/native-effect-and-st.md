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

## Representation prototype (2026-10-12)

The first implementation step, "validate shared generic representation and typed
requests", has a working prototype. It lives outside the package, as the
implementation order requires, and is exercised by a compiler regression test
so a future change cannot silently break the encoding.

The prototype supplies a rank-N heterogeneous request functor and a free
computation over it:

```purescript
type Methods next z =
  { new   :: forall a. a -> (Cell a -> next) -> z
  , read  :: forall a. Cell a -> (a -> next) -> z
  , write :: forall a. Cell a -> a -> (Unit -> next) -> z
  , abort :: forall a. a -> z
  }

newtype NativeF next = NativeF (forall z. Methods next z -> z)
data Native f a = Return a | Suspend (f (Native f a))
```

`next` is the continuation result, so a request preserves the relationship
between a cell's element type, the operand, and the continuation result without
a GADT or an unsafe payload cast. `runNative` interprets one `Suspend` at a time
and tail-calls the continuation, so interpretation runs in constant stack; cell
storage uses the existing checked `arrayFill`/`arrayWrite`/`arrayIndex`
primitives, and `abort` uses the checked storage trap.

Evidence:

- `tests::native_representation::a_rank_n_heterogeneous_storage_and_host_program_runs`
  compiles the module set and an entry that allocates an `Int` cell and a
  `String` cell, writes both in place, reads both, reads the WASI monotonic
  clock through a `Host` request, and returns `42` only if the String read is
  `"b"` and the Int read is `41`. The program uses `do`
  notation, so the `Functor`/`Bind`/`Apply`/`Applicative`/`Monad` instances are
  exercised too.
- The whole workspace keeps the same four failing targets and 71 failed names
  with no added names (`workspace-after-native-proto.log`).

### Host leaves

A WIT import declared as `Effect X` cannot satisfy the ABI: `core_matches_result`
compares the import's resolved result against its canonical result and does not
unwrap `Effect`, so the checked `getStdin :: Effect InputStream` and its
neighbours fail. A raw host leaf declared at its canonical result instead
(`foreign import "wasi:clocks/monotonic-clock#now" clockNow :: Int`) validates
and executes; the interpreter turns it into a `Host` request and performs the
call. The prototype's `Host` family has one operation and the interpreter calls
the raw leaf once per request.

### Recorded blocker: result-role representationality

The official `Effect` module declares `type role Effect representational`, and
`ST.Global.toEffect` is `unsafeCoerce` under that contract, so the shared
representation must infer a representational result role. The naive free monad
`data Native f a = Return a | Suspend (f (Native f a))` does not: `a` sits inside
`f`'s parameter, whose role is unknown at `Native`'s definition, so `a` is
inferred **nominal**, and `type role Effect representational` over it is
`RoleMismatch`.

The direction is confirmed, not just hypothesised. Upstream `Free` erases its
result type internally
(`data Free f a = Free (FreeView f Val Val) (CatList (ExpF f))`), so `a` is
phantom there; a `newtype Effect a = Effect (Free NativeF a)` with
`type role Effect representational` type checks. The design's chosen
`Run (native :: NativeF) a` is `Run (Free (VariantF r) a)`, so it inherits that
representational result role. The work is therefore to express the storage and
host requests over the `Free`/`Run` answer-indexed representation (whose
`perform` carries the operation answer, not the continuation result), rather
than over the naive continuation-passing free monad. That is the next design
step; the public-module migration does not proceed on the nominal encoding, and
the invalid role annotation is never forced.

What the prototype does **not** do, and what the checklist still requires: there
is no `ST` facade, region boundary, `ST.Global` conversion, safe mutable-array
integration, or public-module migration, and no `Effect` module with the
official role annotation or exports. Every checklist row remains Unverified
except that NE-04's heterogeneous request typing now has prototype evidence for
two element types.
