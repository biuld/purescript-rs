# Recursive initialization error investigation

**Recorded:** 2026-10-25 (Asia/Shanghai)\
**Status:** Root cause and exception transport verified; production fix open.\
**Owner:** #101 / RV-08 in [recursive local values](recursive-local-values.md)\
**Contracts:** [CC IR](../../design/backend/fp/cc-ir.md),
[Effect.Exception](../../design/backend/fp/effect-exception.md)

## Distinguish the failures

The earlier scalar regression returned from its initializer but skipped the
ready-state write during tail conversion. Commit `656611c0` repairs that
continuation defect; the maintained scalar execution returns 42.

The remaining trap has a different path. A valid source initializer passes a
thunk referring to its own value into a function that immediately calls it:

```purescript
applyThunk :: (Unit -> Int) -> Int
applyThunk thunk = thunk unit

bad :: Unit -> Int
bad _ =
  let value = applyThunk (\_ -> value)
  in value
```

The outer getter writes Initializing (1), enters the initializer, then the
thunk calls the same getter before the initializer returns. No ready-state
write should have occurred yet. The compiler must report initialization demand,
not fabricate a result, retry the initializer, or treat the state as corrupt.

Current `cc/lower/letrec/values.rs` emits explicit arms for Uninitialized (0)
and Ready (2), and an `Unreachable` default. Initializing (1) consequently uses
an engine trap. `Effect.Exception.try` selects the native source-error tag and
correctly does not catch an engine trap. That catch policy is not the defect.

## Actual state observation

Artifacts are retained in `/tmp/psrs-stream/trap-analysis/`. The frozen compiler
is `psrs-v2-final`, SHA-256
`321521255dbb46ea5888e6448e9c57d95baf38cb2a53265795e2a6e247b3af94`.
The original WAT execution exits 134 with an unreachable trap. Its innermost
frame is function 439, the same getter also present in the next outer frame.

Two diagnostic-only WAT variants discriminate the state reaching its default
arm. A matching tag deliberately traps with `unreachable`; a nonmatching tag
performs an intentionally out-of-bounds access on an empty private array:

| Diagnostic condition | Actual outcome |
| --- | --- |
| Tag equals 1 | Unreachable trap, exit 134 |
| Tag equals 0 | Array-bounds trap, exit 134 |

`state-probe.json` records the original/variant hashes and complete streams.
This establishes tag 1, rather than merely inferring it from an undifferentiated
trap. The variants change no source or compiler code and are not acceptance
artifacts or a proposed implementation.

## Existing exception transport works

The source imports a live `try`, so its emitted main module already has:

- source-error tag 0 with one non-null GC String payload;
- independent stack-exhaustion tag 1 with no payload.

A diagnostic WAT prototype replaces only the getter's trap with a String payload
and `throw 0`. The unchanged ordinary source `try`/`message` path then prints
`probe: recursive value demanded during initialization`, exits 0, and has empty
stderr. A negative control using `throw 1` exits 1 with no output: source `try`
does not recover that distinct tag. Full results are in
`source-error-prototype.json` and `other-tag-control.json`.

This proves transport and dynamic protection for this program. It does not
implement compiler lowering, plan a tag for programs without a live catch, or
establish source-location/message fidelity. The prototype's diagnostic message
is deliberately not presented as the official error. No production file or
independent stdlib source was changed for these experiments.

## Read location is required

The official purs/Node comparison with a multiline initializer separates its
declaration (line 11) from its recursive read (line 13). The error message names
line 13. `ReadSite.purs`, `readsite-official-build.json`, and
`readsite-node.json` retain the actual source, compiler invocation and output.

Current getter calls have no explicit parameters. `read_local_binding` sees
the read expression's range, but the generated getter only retains the binding
range. Raising a fixed message inside it would lose the failing-read location.
A later read of an escaped getter after an initializer failure also needs its
own read location; it must not reuse an initializer error or retry the work.

General provenance must survive Core linking and P7 inlining before rendering
that location. The independently reproduced `origin-probe` already attributes
Helper's bytes 153..165 to a 58-byte Main file. Inferring a location from the
current enclosing function therefore gives a provably wrong result. This is a
shared representation problem, not a recursion-only lookup-table problem.

## Required implementation boundaries

1. Preserve authoritative source identity/range through linking, substitution,
   inlining, capture/lifting and lowering. Keep synthetic IR origins explicit;
   never invent a real module or line when that information is absent.
2. Extend the common getter-force contract to retain the failing read's context.
   Keep eager initialization, shared identity and no-retry behavior unchanged.
3. Add a generic checked source-error raise at the CC/native boundary, rather
   than manufacturing an unchecked library import. Require a String payload
   and a bottom result; preserve spans and verify malformed operands.
4. Share source-error tag planning with existing checked throw/catch providers.
   A reachable compiler-generated raise must work even without a live catch or
   explicit source throw. Keep the exception capability check and the distinct
   stack-exhaustion channel.
5. Give Initializing an explicit source-error arm. Impossible state tags retain
   their unreachable guard; assertion and invalid-resumption traps do not become
   catchable as a side effect of this repair.
6. Execute the accepted dynamic catch/message comparison, distinct read-site
   cases, failed-initializer/no-retry and escaped-getter cases, disabled-profile
   and malformed-IR checks, the existing seven recursive-value cases, Aff
   regressions and required workspace validation. Re-measure the runtime board
   before recording a new gate number.

The established Error target remains message-only, default-name Error and
stackless, as specified by DEC-24. The transport prototype does not extend that
representation or claim equality with JS ReferenceError names or stack traces.
