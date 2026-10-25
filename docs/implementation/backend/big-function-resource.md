# BigFunction compiler resource failure

**Recorded:** 2026-10-25 (Asia/Shanghai)\
**Status:** Open; compiler stage and allocation owner are not established.\
**Related:** [runtime gate](../../design/D-04-suite-roadmap.md#m7--runtime-and-standard-library),
[Core type compaction](core-type-compaction.md),
[compiler iteration SOP](../../workflow/compiler-iteration-sop.md#bounded-runtime-measurements)

## Confirmed observation

This is a compiler-process allocation failure, **not a Wasm execution trap**.
The latest complete runtime measurement gives `BigFunction.purs` an incomplete
compiler verdict. It must not be counted as a source rejection, a runtime
failure, or a supported program. No guest execution was observed for this case.

| Item | Retained evidence |
| --- | --- |
| Input | `tests/upstream/passing/BigFunction.purs`: 140 lines, 407884 bytes; longest line 6726 bytes |
| Input SHA-256 | `5fb4802baf4db29c65875bfeafc17de9db7c7549bf847838f80c4e8c21dd3b4f` |
| Compiler address-space limit | 8589934592 bytes (8 GiB), applied only to the compiler process |
| Compiler outer timeout | 90 seconds |
| Observed result | Process status -6; 36.244 seconds; `memory allocation of 56 bytes failed` |
| Runtime harness SHA-256 | `3cc5c3e2085e649518762515afe1c221ffa67ebbae26f34fc7266c41131ddecd` |
| Library lock | `b817f3daf34247360216b48f10c14d8d14ccca3e` / `fnv1a64-v1:acd7fdd2f2a78577` |
| Measurement | `/tmp/psrs-stream/runtime-board-final/{measurement,results,summary,terminal-status,acceptance-audit}.json` |
| Raw case log | `/tmp/psrs-stream/runtime-board-final/BigFunction.purs.log` |

The complete 365-root run finishes successfully as a measurement, with this
case explicitly marked `incomplete_compilation` and
`resource_failure_observed`. Its other results are 316/340 agreement, 25 FFI
exclusions and no lost agreement. The original Aff-integration baseline also
records an incomplete allocation failure; the recursive-value change does not
claim to repair it.

The reported 56 bytes are the **last failed allocation request**, not total
memory use or the size of the problematic representation. An address-space
limit also does not establish peak resident memory. The retained log has no
owning-stage backtrace or completed-pass record, so it cannot identify a Core,
solver, pattern, or backend allocation as the cause. In particular, the earlier
million-node unused Core type-table defect is not proof of this case's cause.

## Reuse before rerunning

Read the retained result and log first. The frozen executable and full archived
source snapshot are retained under `/tmp/psrs-stream/`; commit `656611c0` contains
the measured production source. Its measurement was built before that commit
from the same byte-verified source tree.

If a new observation is needed, use a single-root bounded run, not another
complete scoreboard:

```sh
python3 crates/psrs-driver/tests/suite/run_runtime.py \
  /tmp/psrs-stream/suite-v2-frozen \
  --repo /tmp/psrs-stream/purescript-rs \
  --case BigFunction.purs \
  --out /tmp/psrs-big-function-follow-up
```

Use a new output directory. Keep the existing compiler-only 8 GiB / 90-second
bounds and distinguish any instrumented build from the frozen baseline.
The runner executes this case after its worker pool exits and removes the
compiler address-space bound from a guest engine if compilation ever succeeds.

## Required next investigation

1. Capture the last completed compiler stage and allocation owner under the
   same bound, with a small pass-boundary trace or a bounded profiling run.
2. Measure peak memory and identify which retained representation grows; do not
   infer either from the final allocation-request size.
3. Repair the owning shared representation or operation while retaining full
   checking and the unchanged official source. Do not bypass it by changing
   guest trap handling, shrinking the input, or weakening the runtime gate.
4. Re-run this exact root with comparable inputs and limits. Only actual
   successful compilation and execution establish recovery; otherwise preserve
   the precise incomplete verdict and new evidence for continuation.

No further BigFunction run was performed merely to write this record.
