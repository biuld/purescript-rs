# F-04 — Compile Diagnosis

**Status:** In progress

**Design:** [D-16 — Compile Diagnosis](../design/D-16-compile-diagnosis.md)

## User need

Compiler contributors need a repeatable way to identify where a source case
stops, compare compile behavior across revisions, and replay an investigation
without reconstructing its module inputs. When a pipeline boundary changes,
they need evidence tied to the actual artifacts and passes that ran.

## User-visible behavior

Given one source file or a selected part of the `passing` corpus, contributors
can run `psrs diagnose` to create a versioned report. The report lists each
diagnostic and its source origin, groups cases by first-blocker stage and
category, and records exclusions, timeouts, crashes, and elapsed time
separately from successful compilation.

The default report keeps a lightweight manifest of pass executions, artifact
identities, and explicit input/output edges. `--trace` retains selected IR
dumps on both successful and failed cases and carries the option into replay.
Unsupported artifact summaries or lineage are identified as unavailable. A
report never treats a compile pass as evidence that generated Wasm executes
correctly.

## Acceptance criteria

- A single source file can be diagnosed with a per-case deadline.
- File diagnosis can take additional ordered `--input FILE` sources, compile
  that exact list without import rediscovery, and reject duplicate canonical
  paths or combining explicit inputs with corpus mode.
- The selected corpus subset reports progress and records FFI exclusions without
  counting them as passing cases.
- Every case retains all ordered diagnostics and the origin of each diagnostic.
- The run manifest records the available input, compiler, target, toolchain,
  schema, pass-contract, and trace-coverage metadata without inventing missing
  compatibility evidence.
- Each observed pass has a stable identity, ordered input and output artifact
  references, status, diagnostic references, and explicit artifact edges.
- Artifact production is distinct from verification evidence; a partial,
  rejected, timed out, crashed, or unavailable result is represented
  truthfully.
- Default diagnosis avoids formatting or cloning complete IR values solely for
  dumps. `--trace` can retain the selected dumps for accepted and rejected
  cases. Replay first creates a new trace report from the saved ordered inputs,
  then runs the recorded build command so a compiler rejection keeps its
  nonzero build exit status.
- Reports compare only compatible cohorts and identify changed or incomplete
  source inputs. Where canonical summaries are absent, artifact-content
  comparison is explicitly unavailable.
- Compile acceptance, compile lineage, and Wasmtime runtime evidence remain
  separate; runtime correctness requires value-sensitive execution evidence.
- Compile outcomes remain data in a completed report; command failure indicates
  invalid options or an operational error such as missing input or unwritable
  output.

## Implementation status

Implemented: versioned run-local pass/artifact IO records, observed top-level
backend boundaries, a diagnosis API with optional dump capture, and the CLI
schema-v2 manifest. Default reports keep pass/artifact metadata without
formatting IR dumps; `--trace` retains selected Core, CC, and MIR dumps on
accepted and rejected cases. Explicit ordered `--input` sources and trace-aware
replay are supported. `--compare` retains v1 status/blocker/input comparison
and reports the first observed pass-sequence difference for compatible v2
traces. It reports observed environment differences separately and marks the
compiler build toolchain unavailable because it is not embedded in the
binary. Exact nested-verifier coverage is recorded at the granularity exposed
by the backend.

Planned: canonical versioned IR summaries and artifact-content comparison,
source/declaration/node lineage, many-to-many derivation queries by
case/symbol/span, and a separate Wasmtime runtime evidence record. Current
pass comparison follows execution order and does not establish the earliest
artifact divergence or explain a runtime failure. These capabilities are not
implied by the presence of a pass manifest or an IR dump.

## Out of scope

This workflow does not repair compiler behavior, infer lineage from Debug
output or span overlap, reduce a source file automatically, prove matching
diagnostics have a common root cause, or use compile acceptance as runtime
correctness evidence.
