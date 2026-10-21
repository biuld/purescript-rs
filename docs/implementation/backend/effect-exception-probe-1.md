# Effect.Exception probe 1: diverging throw destinations

**Date:** 2026-10-21\
**Scope:** The first of the four pre-implementation probes for stage 3.\
**Owning contracts:** [MIR](../../design/backend/fp/mir.md),
[exception channel](../../design/backend/fp/exception-handling-and-stack-exhaustion.md),
and [Control exception bridge](../../design/backend/fp/control-exception-bridge.md).

## Result

Keep direct discharge into `Terminator::Throw`. No `Instruction::Throw`
fallback or MIR representation change is needed for the tested control-flow
forms. A throw's result declaration may remain without a definition because
there is no normal use on the terminated path. This also works when the
function's declared return type is a non-null GC String reference.

Two integration changes are required:

- Stop lowering a sequence after its block terminates. Append normal function,
  if/switch arm and protection-region exits only to open blocks; keep the
  existing strict duplicate-terminator check for other callers.
- Resolve local throw handlers after the function CFG is complete using the
  existing shared try-region partition. The proposed unconditional
  `handler: None` is valid only when no enclosing clause catches the tag.
  A protection handler is outside its own disarmed region.

No verifier, structurer or encoder relaxation is involved.

## Baseline and ownership

The starting compiler revision was `58b2424ce6546495dbf9a58e7362b32c571fa48d`;
the checkout already contained uncommitted stage 2 changes and partial stage 3
provider validation, tag planning and throw/catch discharge. The authoritative
revision and initial diff are saved under `/tmp/opencode/stage3/probe-1/`.

The first test build exposed an existing fixture initializer missing the new
`RuntimeContext.exception_tags` field. After adding its empty inventory, the
CC probe reached P9 and failed on `Tail/Integer` with `basic block already has
a terminator`. After repairing normal exits, `Protect(false)/Integer` failed
P9 verification with `MIR throw must name the matching handler of the innermost
enclosing try region`. These failures locate both defects at the CC-to-MIR
producer boundary. The verifier's requirements remain unchanged.

This is a hand-built CC reproducer, using the existing checkThrows discharge
fixture. It does not exercise source parsing or Core elaboration, so no
source-level diagnose comparison or official-suite score is claimed.

## Focused evidence

Test: `mir::tests::exception::throw_destination::throw_destination_probe`.

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend --lib \
  throw_destination_probe -- --nocapture
```

The final run executed 36 observations: nine sites, two return shapes
(`Integer`, `String`), and two paths (MIR optimization bypassed and enabled).
Every case passed MIR verification, Wasm lowering, binary validation,
component composition and execution under Wasmtime 48.0.1 with
`-W max-wasm-stack=524288`. There were no skipped runtime observations.
The checkThrows wrapper reports a caught throw as exit 42 and normal return
as exit 0; exit 1 would instead expose an uncaught error. Stdout and stderr
must both be empty.

| Site | Observed exit in both return shapes and paths |
| --- | --- |
| Function-tail throw | 42 |
| Throw followed by an ordinary assignment | 42 |
| If selecting its throwing arm | 42 |
| If selecting its returning arm | 0 |
| If with both arms throwing | 42 |
| Switch selecting its throwing case | 42 |
| Switch selecting its returning default | 0 |
| Throw in a protection body, repropagated by its handler | 42 |
| Throw again in a protection handler | 42 |

The test also checks that lowering does not fabricate an instruction defining
the throw destination. Logs are `baseline.log`, `after-exit-fix.log` and
`after.log` under `/tmp/opencode/stage3/probe-1/`.

## Workspace validation

- `cargo fmt --all --check`: passed (`fmt.log`).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed (`clippy.log`).
- `PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`:
  2,231 unit/integration tests passed, zero failed, five scoreboards ignored.
  The command subsequently failed during backend rustdoc compilation with
  `E0463: can't find crate for psrs_linker` (`workspace.log`). This occurred
  after additional Cargo builds ran while the suite's test processes were
  executing, consistent with replacement of dependency artifacts.
- Serial retry with the same environment and `cargo test --workspace --doc`:
  passed for every workspace crate (`doctests-retry.log`). No source change
  was needed to recover rustdoc. The full workspace command itself is
  recorded as failed, with its failing layer recovered separately.
- The focused probe was rerun after strengthening its caught-exception exit
  sentinel from 1 to 42; all 36 observations passed (`after.log`).
- `git diff --check`: passed.

## Remaining scope

This establishes the diverging-destination lowering only. Rank-2 `stackImpl`,
the named catch path with a String payload after final type-index remapping,
and trusted-module insertion effects remain separate probes. It does not
complete stage 3, establish public `Effect.Exception` API conformance, or
change a roadmap measurement. Package sources and the compiler lock were not
changed by this probe.
Package conformance, the JS oracle, the explicit purs differential and the
ignored scoreboards were not rerun for this CC-to-MIR probe.
