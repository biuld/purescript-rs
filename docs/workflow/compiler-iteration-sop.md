# Compiler Iteration SOP

Use this workflow when investigating compile failures or making a compiler
change. It creates reproducible evidence, locates the earliest observed
boundary, and checks the same inputs after a change. Compile evidence and
runtime evidence answer different questions and must be recorded separately.

For roadmap work, first select an issue using the phase and dependency rules in
[`AGENTS.md`](../../AGENTS.md#choosing-the-next-item), then read its references
and validation requirements. Diagnosis helps investigate the selected work; it
does not replace roadmap priority or acceptance evidence.

## 1. Record the starting point and capture a baseline

Record the branch, commit, and existing worktree changes. Choose one reproducer
or a small corpus cohort that reaches the relevant feature. Save reports and
bundles outside the source tree so they do not change the working-tree
fingerprint in a later report.

```sh
# One file, including imported modules and a replay bundle.
cargo run -p psrs-cli -- diagnose path/to/Main.purs \
  --out /tmp/psrs-before.json --timeout 20

# A bounded, repeatable corpus cohort. The filter is applied before the limit.
cargo run -p psrs-cli -- diagnose --corpus passing --filter Functor --limit 20 \
  --out /tmp/psrs-before.json --timeout 20
```

Use the default run first when pass outcomes and blocker attribution are enough.
Compiler rejections are recorded in the report and do not make the diagnosis
command fail. A nonzero command result indicates an operational problem such as
invalid arguments, missing inputs, or an output error. Timeouts, crashes, and
FFI exclusions are separate outcomes; none counts as a successful compile.

## 2. Locate the first observed boundary

Start with the case's first blocker and complete ordered diagnostics. Keep
source, trusted-library, and program-level origins distinct. A library-origin
diagnostic may be triggered by a user module, but does not establish which
implementation is wrong.

The run manifest records actual pass executions and their input/output
artifacts. Follow those edges to find the first observed boundary that differs
from the expected contract. A matching first-blocker group is only a shared
diagnostic signature. Use group size to estimate reach, then inspect
representatives before treating one owner hypothesis as established.

For a focused case where the IR itself is needed, request a trace:

```sh
cargo run -p psrs-cli -- diagnose path/to/Main.purs --trace \
  --out /tmp/psrs-before-trace.json --timeout 20

# An explicit ordered input closure for a multi-file reproducer.
cargo run -p psrs-cli -- diagnose path/to/Main.purs \
  --input path/to/Imported.purs --trace \
  --out /tmp/psrs-before-trace.json --timeout 20
```

Trace mode retains the selected Core, CC, and MIR dumps for accepted and
rejected cases and is preserved by the replay bundle. Open only artifacts whose
manifest record says they were produced. Read validation records separately:
artifact production does not establish verification. Pass entries may group
nested verifier work at an explicitly coarser observed boundary. A missing
summary or lineage relation means that evidence is unavailable; do not
reconstruct it from formatted text or overlapping spans. Querying by case,
symbol, or source span is planned; until it is implemented, inspect the saved
case, pass, and artifact records directly.

`--compare` requires matching cohort selection and trusted-library fingerprints.
For each matching case it separately checks complete input fingerprints before
comparing pass records. The reported first difference is the first observed
execution-sequence difference, not an earliest artifact divergence or a root
cause. Canonical artifact-content comparison is unavailable until versioned
summaries exist. Host and installed tool versions are observations only; the
compiler build toolchain is not embedded in the current manifest.

Use repeatable `--input FILE` when the reproducer's source set is explicit.
The primary file is first and each `--input` follows in order; this mode
compiles that exact list without resolving imports again. It applies only to
file diagnosis, and duplicate canonical paths or `--corpus` combined with
`--input` are invalid. A trace replay runs `diagnose` on the saved entry-first
list into a new `replay-trace.json`, then invokes the recorded `build` command
on the same files. The two runs are separate; the build command retains the
original compiler rejection's nonzero status.

Failure bundles record the case and loaded inputs; `replay.sh` reruns with the
recorded compiler and trace mode. Override the executable when comparing a
different build:

```sh
BUNDLE=/tmp/psrs-before.bundles/0000-Functor
PSRS_BIN="$PWD/target/debug/psrs" sh "$BUNDLE/replay.sh"
```

Use the bundle directory recorded in the report; the example name above is
illustrative.

## 3. Reduce the case and identify the owner

Make a small reproducer from the replay bundle while preserving required
imports and declarations. State the expected behavior, actual diagnostic or
output, and the earliest representation where they diverge. For generated
adapters or other compiler-created nodes, use explicit derivation records when
available. Source span overlap is a search hint, not lineage evidence.

Read the governing design and identify the stage that owns the rule. Check the
input and output invariants of adjacent stages. Prefer repairing a shared
representation or operation when multiple forms rely on it. Avoid a
feature-specific backend workaround when an earlier representation or calling
contract is wrong. Keep a falsifiable next hypothesis; when an investigation
produces no next step, save the evidence and narrow the unanswered boundary
before adding speculative changes.

For roadmap work, the issue and board rules still decide which work comes next.
Among cases within that work, use affected-case count and downstream
dependencies to choose investigation order; a large signature group alone
does not prove cause or raise an issue's roadmap priority.

## 4. Validate the change at each affected layer

Run a focused test or replay that covers the reduced case and relevant
rejection path. Then rerun the same diagnosis selection with the same filter,
limit, timeout, corpus, trusted-library setup, and trace mode:

```sh
cargo run -p psrs-cli -- diagnose --corpus passing --filter Functor --limit 20 \
  --out /tmp/psrs-after.json --timeout 20
cargo run -p psrs-cli -- diagnose --compare /tmp/psrs-before.json /tmp/psrs-after.json
```

Comparison checks cohort and observed input compatibility, then reports
per-case recovery, regression, blocker changes, input changes, and unmatched
paths. Compiler revision may differ and is shown as the subject of comparison.
If inputs are incomplete or environment metadata differs, keep that limitation
visible. Until compatible canonical summaries are available, artifact-content
and earliest-artifact difference are reported as unavailable; do not infer an
IR change from different Debug output.

Compile diagnosis establishes compile acceptance only. If behavior reaches
Wasm execution, run a focused value-sensitive runtime check that observes the
relevant results, call counts, output order, or traps. Record the exact Wasm
artifact and runtime/tool version where the test supports it. Keep outcomes
separate, for example:

```text
compile: accepted
runtime: trapped
value assertions: incomplete
```

A successful compile does not establish correct runtime behavior. For
runtime-gated driver tests that support it, require Wasmtime explicitly:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib focused_runtime_test_name
```

Run the issue's required official-suite or scoreboard validation when the
change affects a gate or its acceptance evidence. Update D-04 and README
measurements only after rerunning the relevant scoreboard with the documented
settings. Small filtered diagnosis reports are useful for locating regressions;
they are not replacements for official acceptance measurements.

## 5. Report the evidence

For each iteration, report:

- the starting commit and whether the worktree already had changes;
- the exact baseline and comparison commands, selected cohort, and trace mode;
- compile outcomes by first-blocker stage/category, keeping exclusions,
  timeouts, crashes, and incomplete inputs separate;
- the first observed pass/artifact boundary, its evidence coverage, the
  reproducer, and the suspected semantic owner;
- focused compile checks, runtime behavior exercised, and required suite or
  scoreboard runs;
- what changed, what remains unresolved, and which validations were not run.

Do not turn a filtered cohort into a corpus-wide pass rate. Do not update a
scoreboard number from diagnostic output or from an unverified estimate. A
manifest, dump, or compile status does not substitute for missing lineage,
canonical artifact summaries, or value-sensitive runtime evidence.
