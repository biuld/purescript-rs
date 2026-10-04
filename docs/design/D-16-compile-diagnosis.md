# D-16 — Compile Failure Diagnosis

**Status:** Draft

**Implements:** [F-04 — Diagnose Compile Failures](../feature/F-04-compile-diagnosis.md)

## Purpose

`psrs diagnose` runs one source file or a selected part of the vendored
`passing` corpus through the normal compiler pipeline. It records the first
blocking stage and category for each case, keeps every diagnostic with its
source origin, and writes a versioned JSON snapshot that can be compared with a
later compiler revision. Failure bundles retain the exact loaded user modules,
the replay arguments, and every IR stage that completed successfully.

## Commands

```sh
cargo run -p psrs-cli -- diagnose path/to/Main.purs --out /tmp/main.json
cargo run -p psrs-cli -- diagnose --corpus passing --filter Functor --limit 20 \
  --out /tmp/functor.json --timeout 20
cargo run -p psrs-cli -- diagnose --compare /tmp/before.json /tmp/after.json
```

The corpus command uses `tests/upstream` unless `PURESCRIPT_REPO` selects an
official checkout. It applies the path filter before the limit. Cases with
foreign imports or adjacent JavaScript FFI files are recorded as `excluded`;
they do not count as passes.

Each selected case runs in its own child process. `--timeout` is a per-case
deadline in seconds. A timeout or crash is captured as a case result, and the
batch continues. Progress and elapsed time are printed to stderr. Compiler
rejections are snapshot data and do not make the diagnosis command itself fail;
invalid options, missing files, or report I/O errors do.

## Snapshot and comparison

The JSON snapshot records the selected cohort, the trusted-library fingerprint,
the compiler commit, dirty-tree fingerprint and executable fingerprint, every
case's input fingerprint and elapsed time, and its full ordered diagnostic list.
Source, trusted-library and program-wide origins remain distinct. A failed
backend attempt includes the last successful Core, verified CC, and MIR dumps
available from the shared compile pipeline; an unverified CC candidate is never
presented as a completed stage.

Summary groups use first-blocker stage and stable diagnostic category. The full
message and span stay on each case, and a shared group is only a matching
diagnostic signature; it does not claim the cases share one root cause.

Snapshots compare only when corpus mode, selected path filter and limit,
timeout, and trusted-library fingerprint match. Compiler revision may differ.
For cases with complete input sets, changed module contents are reported as
`input_changed`. If a worker timed out or crashed before loading all modules,
the case is still compared by observed status and the report marks input
comparison as incomplete. New and removed paths are listed as unmatched rather
than counted as regressions or recoveries.

Each failed case gets a directory beside the snapshot with numbered source
files, `case.json`, and `replay.sh`. The bundle records whether module loading
completed. `core.debug`, `cc.debug`, and `mir.debug` are present only when the
named stage completed; the adjacent `.stage` file gives the producing pass.
`replay.sh` runs the same CLI binary recorded in the bundle by default. Set
`PSRS_BIN` to choose another compiler executable.

## Scope

This tool measures compile acceptance and locates the first compiler-reported
blocker. It does not execute generated Wasm, diagnose runtime traps, reduce a
source file automatically, or prove a grouped failure has a common cause.
