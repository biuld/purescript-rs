# Library-owned effects: scope and implementation audit

Date: 2026-10-11. Compiler branch: `stdlib/vendor-core-libraries`, HEAD
`6b40b8d6`. This audit includes the existing 28 modified/untracked paths,
including the uncommitted scoped-callback and zero-width-choice slices. The
independent development package is consumed explicitly; the normal compiler
lock remains at `1e01310daa7e56de5ed379029d6a24dcf39e879c`.

## Conclusion

The primitive State representation and small runtime boundary fit the selected
goal: ordinary library Effect/ST newtypes, library combinators, and checked
target operations. They do not require compiler recognition of the new Effect
constructor. The implementation and continuation plan have nevertheless grown
into a partial general State transport and provenance system. Completing that
system has become a prerequisite for the package migration without a concrete
public API blocker for every extension.

The most urgent reproduced problem is in an actual public API: a 10,000-iteration
`Effect.forE` program compiles but exhausts the Wasm call stack. Expanding raw
State closure transport does not resolve that acceptance gap. The safe response
is to prioritize public package behavior and constrain the remaining work by
real reproducers, while preserving sequencing checks and honest rejection paths.

## Necessary owners

| Owner | Reviewed responsibility | Assessment |
| --- | --- | --- |
| Library | Effect/ST newtypes, pure/bind, callbacks, loops, Ref wrappers, command runner | Matches the goal. Algorithms remain ordinary source. |
| Primitive catalog | State/RealWorld identities, nominal region role | Necessary for the selected primitive State design. |
| Intrinsic catalog | Only `RunWorld` and `RunRegion`, with checked signatures | Bounded execution boundaries; no Effect/Ref algorithm intrinsic. |
| Runtime | GC storage fill/read/write and non-returning trap | Bounded executable primitives. Physical ABI has no State token. |
| Compiler call boundary | Complete source signatures, region agreement, payload representation, host/runtime ABI projection | Necessary. Newtype and zero-width calls affect HIR/Core/CC/MIR even though execution lives in runtime. |
| Dependency preservation | Keep observable calls ordered and retain evidence until checked erasure | Necessary correctness obligation; its current general source interpreter is a design choice, not dictated by the newtype equation. |

Relevant owners are `psrs-hir/src/intrinsic/state.rs`,
`psrs-core/src/state/primitive.rs`, `psrs-runtime/src/abi/storage/mod.rs`,
`psrs-runtime/src/storage/mod.rs`, and the backend call/projection boundaries.

GHC's IO representation uses primitive State# and a RealWorld marker; State# has
no runtime representation. This supports the selected representation direction.
It does not establish that this compiler's lexical callback interpreter and
single-root graph restrictions are required by Haskell's IO definition. Our
State is exposed at kind `Type`, whereas GHC documents State# as unlifted; the
source restrictions here require their own explicit contract.
References: [GHC IO](https://downloads.haskell.org/ghc/latest/docs/libraries/ghc-internal-9.1401.0-555c/GHC-Internal-IO.html)
and [GHC primitive State#](https://hackage-content.haskell.org/package/ghc-prim-0.12.0/docs/GHC-Prim.html).

## Findings

### A1: Public loops exhaust the stack (high priority, reproduced)

The development package implements `forE` recursively through ordinary bind.
The backend's `mir/lower/tail.rs:24` returns immediately for any function with
State evidence, bypassing both self-recursion loopification and tail-call
conversion. State functions are also excluded from several optimization passes
in `mir/opt/{inline,values,cfg}.rs`. These are explicit temporary guards, not
silent successful-looking placeholders, but they impose real costs on normal
library code.

Audit program:

```purescript
main :: Int
main = unsafePerformEffect do
  forE 0 10000 (\_ -> pure unit)
  pure 42
```

The program compiles successfully and Wasmtime 49.0.2 exits 134 with
`wasm trap: call stack exhausted`. The same program with 100 iterations is
accepted and exits 42. The clean pinned official Effect.js implements `forE`
as a JavaScript loop and completes 10,000 callbacks. The recursive target
implementation and disabled tail transformations are implicated by inspection;
this audit did not isolate their individual contributions by changing code.

Disposition: make stack-safe execution of actual public loops and MonadRec
cases a package acceptance obligation. Repair the owning lowering/transfer
contract or select a justified library/runtime primitive decomposition; do not
substitute a whole-function Effect-loop intrinsic merely to bypass the issue.
Earlier short-loop conformance remains valid for its measured inputs, but does
not demonstrate stack safety.

### A2: Raw State capture support depends on source arrangement (medium priority, reproduced)

Three probes perform the same storage read through a closure capturing the
current raw State. Direct invocation compiles and executes to 42. Routing the
closure through a local callback helper rejects at P8; routing it through a
top-level helper rejects at P7.

Core's `state/flow/closures.rs` interprets registered lexical lambda bodies with
a captured producer environment, treating unknown call targets conservatively.
CC's `state/captures.rs` permits one direct State capture and no explicit State
parameter. The supported subsets differ across stages. This is partial general
higher-order transport, rather than merely an execution/storage intrinsic.

These probes are deliberately low-level State programs. This audit did not find
a migrated public Effect API requiring this exact raw-State-capturing helper
shape. Ordinary bind passes the successor to the next action as an explicit
argument. Public Effect/ST/Ref evidence predates the current capture extension.

Disposition: defer further generic capture expansion until an actual package
reproducer requires it. Keep the current unsupported forms explicit; do not
relax checks to make them appear supported. If retained as a separate feature,
define one cross-stage invocation contract and test equivalent source forms.

### A3: Signature projection is broader than its stated captured-call justification (medium priority, static concern)

`mir/state/projection.rs:98` rewrites the result of every stored signature using
the structural Step-to-payload map. The nearby comment justifies this by checked
captured callback producer/body evidence, but the loop itself is not restricted
to those producers or signatures. Inventory and subsequent correspondence
checks still exist; this audit has not established an accepted invalid program
or miscompilation from this rewrite.

Disposition: identify the authoritative set of dischargeable call/result
boundaries and project through that checked plan. Alternatively, prove and
document why every structural Step occurrence in this loop has the required
evidence. Keep physical-layout cleanup distinct from semantic admission.

### A4: The continuation order turns generic completion into an Effect prerequisite (scope finding)

The implementation checklist puts general scoped transport, higher-order
calls/captures/loops, and general projection before package completion and legacy
deletion. Core verification runs the State flow checker on every otherwise
checked module; that checker traverses declarations and nested State bodies.
Arrays, casts and unknown calls that carry State have additional restrictions
beyond ordinary PureScript type checking. This is substantial new semantic and
verification machinery, although it is not a complete linear type system.

The selected design legitimately requires preserved ordering. The scope error
is treating full generality of this particular proof machinery as necessary for
finishing the migration, without identifying which public behavior it enables.
The growing Core/CC/MIR callback models and optimizer exclusions compound that
cost. Do not remove dependency checks blindly: removing runtime token values
does not remove sequencing obligations.

Disposition: use public API/host reproducers to select required vertical slices.
Separate additional general raw-State language support from migration closure.
Keep one authoritative semantic fact across stages instead of extending several
independent restricted models for each new example.

### A5: Normal consumption still uses legacy Effect (completion finding)

The lock still selects the old package. `driver/program/effects.rs` retains
trusted Effect discovery and old synthesized combinators/runner behavior;
`driver/program/lowering.rs` selects that route when no ordinary runner is
configured. Legacy StateToken paths remain. Explicit development-package success
does not demonstrate migrated normal locked consumption.

Disposition: complete coherent public API/host acceptance, remove the legacy
mechanism, and then update the lock in the established order. Do not change the
lock during this audit.

## Explicit trust boundary

The target-private `PSRS.Effect.fromState` uses `unsafeCoerce` to construct the
hidden ordinary Effect newtype from its exact field representation.
`Effect.Unsafe` similarly unwraps it for the runner. This is a library-owned
representation assumption, not a proof supplied by the compiler. Its signatures,
hidden-constructor policy and runtime suspension behavior must remain recorded
and tested. It is not a fabricated implementation or a per-module compiler hack.

## Disposition and evidence limits

Retain the newtype direction, State/RealWorld identities, two runner intrinsics,
four storage operations, alias-preserving GC arrays, checked payload conversions,
rank-N region checks and state-aware host ABI adaptation. None of those should
be rolled back simply because several compiler stages consume them.

Prioritize public loops, full API/host behavior, optimizer transfers required by
those programs, and coherent legacy removal. Defer broader raw-State capture
generality unless a concrete public use requires it. Revisit the signature-wide
projection contract before enlarging the admitted subset.

This audit changed no compiler or package implementation and ran no full Rust
suite, scoreboard, formatting or Clippy cohort. It inspected the normative
design, checklist, primitive/Core flow, CC capture verification, MIR projection
and optimization guards, runtime ABI/provider, driver legacy/runner paths, and
development-package Effect/ST/Ref/host wrappers. This is an architecture and
focused behavior review, not a proof of complete host/API conformance.

Focused artifacts live in `/private/tmp/psrs-effect-state-audit-20261011/`:
`Direct.purs`, `LocalHelper.purs`, `GlobalHelper.purs` and their build logs;
`PublicLoop/run.json` (compile 0, runtime 134);
`PublicLoopSmall/run.json` (accepted, runtime 42); and `official-loop.json`.
The official checkout is clean at
`a192ddb923027d426d6ea3d8deb030c9aa7c7dda` (Effect v4.0.0).
The development package fingerprint is `fnv1a64-v1:6c93c7e357fcc3e3` and the
probed compiler SHA256 is
`11f1b614fb5d85f838308d20f8498a7d2711000f99773b8f23d498aea04a3928`.


## Tail-call follow-up

After this audit, the user selected enabling tail calls in the default target.
The shared tail pass now admits State functions through checked transfer, and
the original public forE case succeeds at both 10,000 and 100,000 iterations
without library rewrites. See [the implementation evidence](library-owned-effects.md#default-tail-calls-and-checked-state-transfer-2026-10-11).
A separate generic polymorphic Task adapter case still exhausts the stack;
this follow-up does not close all representation/optimizer obligations or the
Effect migration. Findings above describe the audited pre-fix snapshot.
