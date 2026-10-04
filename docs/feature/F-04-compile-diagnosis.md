# F-04 — Diagnose Compile Failures

**Status:** In progress

**Design:** [D-16 — Compile Failure Diagnosis](../design/D-16-compile-diagnosis.md)

## User need

Compiler contributors need a repeatable way to find which stage stops a source
case, compare compile acceptance across revisions, and replay a failure without
reconstructing its module inputs by hand.

## User-visible behavior

Given a source file or a selected part of the `passing` corpus, contributors can
run `psrs diagnose` to get a versioned JSON report. The report lists each
diagnostic and its source, groups cases by first-blocker stage and category, and
records exclusions, timeouts, crashes, and elapsed time separately from passes.
Each failed case produces a replay bundle with its loaded source files and any
compiler-stage details that completed successfully.

Two compatible reports can be compared case by case. The comparison shows
recovered and regressed cases, stage or category changes, changed source inputs,
and paths that occur in only one snapshot. It identifies cases whose full input
set was not captured before a timeout or crash.

## Acceptance criteria

- A single source file can be diagnosed with a per-case deadline.
- The selected corpus subset reports progress and records FFI exclusions without
  counting them as passing cases.
- Every case retains all ordered diagnostics and the origin of each diagnostic.
- A failure bundle contains exact loaded user modules, replay arguments, and
  dumps only for stages that completed successfully.
- A worker timeout or crash is isolated to its case, retains available process
  output, and does not stop the remaining selected cases.
- Snapshot comparison rejects incompatible trusted-library or selection
  cohorts and reports new or removed paths as unmatched.
- Compile outcomes remain data in a completed report; command failure indicates
  invalid options or an operational error such as missing input or unwritable
  output.

## Out of scope

This workflow does not run generated Wasm, attribute runtime traps, reduce a
program automatically, or prove that matching diagnostics share a root cause.
