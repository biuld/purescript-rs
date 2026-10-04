# Compiler Iteration SOP

Use this workflow when investigating compile failures or making a compiler
change. It turns a failing case into reproducible evidence, helps locate the
semantic owner of a defect, and checks the same inputs after a change.

For roadmap work, first select an issue using the phase and dependency rules in
[`AGENTS.md`](../../AGENTS.md#choosing-the-next-item), then read its references
and validation requirements. Diagnosis helps investigate the selected work; it
does not replace roadmap priority or acceptance evidence.

## 1. Capture the starting point

Record the branch, commit, and existing worktree changes. Choose one reproducer
or a small corpus cohort that reaches the relevant feature. Save the report
outside the source tree so the generated JSON and bundles do not change the
working-tree fingerprint recorded in the next report.

```sh
# One file, including its imported modules and a replay bundle.
cargo run -p psrs-cli -- diagnose path/to/Main.purs \
  --out /tmp/psrs-before.json --timeout 20

# A bounded, repeatable corpus cohort. The filter is applied before the limit.
cargo run -p psrs-cli -- diagnose --corpus passing --filter Functor --limit 20 \
  --out /tmp/psrs-before.json --timeout 20
```

Compiler rejections are recorded in the report and do not make the diagnosis
command fail. A nonzero command result indicates an operational problem such as
invalid arguments, missing inputs, or an output error. Timeouts, crashes, and
FFI exclusions are separate outcomes; none counts as a successful compile.

## 2. Find the first blocking boundary

Start with the first blocker stage and category, then inspect the case's full
ordered diagnostics. Keep source, trusted-library, and program-level origins
distinct. A library-origin diagnostic may be triggered by a user module, but
that does not establish which implementation is wrong.

Open a representative failure bundle. Its `case.json` records the case and
inputs; `replay.sh` reruns it with the recorded compiler. Override the executable
when comparing a different build:

```sh
BUNDLE=/tmp/psrs-before.bundles/0000-Functor
PSRS_BIN="$PWD/target/debug/psrs" sh "$BUNDLE/replay.sh"
```

Use the bundle directory recorded in the report; the example name above is
illustrative.

Inspect only IR dumps whose `.stage` file says the stage completed. A missing
dump means that stage did not produce a completed result. Use the last
successful representation to locate where the invariant first stops holding.

Failure groups are matching diagnostic signatures, not proven shared causes.
Use group size to estimate reach, then verify representatives from the group
before treating one owner hypothesis as established. Preserve the distinction
between the number of cases grouped and the number whose root cause has been
confirmed.

## 3. Reduce and identify the owner

Make a small reproducer from the replay bundle while preserving required imports
and declarations. State the expected behavior, actual diagnostic or output, and
the earliest representation where they diverge.

Read the governing design and identify the stage that owns the rule. Check the
input and output invariants of the adjacent stages. Prefer repairing a shared
representation or operation when multiple forms rely on it. Avoid a
feature-specific backend workaround when the earlier representation or calling
contract is wrong. Keep a concrete next hypothesis; when an investigation pass
produces no falsifiable next step, save the evidence and narrow the unanswered
boundary before adding more speculative changes.

For roadmap work, the issue and board rules still decide which work comes next.
Among cases within that work, use affected-case count and downstream dependencies
to choose investigation order; a large signature group alone does not prove
cause or raise an issue's roadmap priority.

## 4. Validate the change at each affected layer

Run a focused test or replay that covers the reduced case and the relevant
rejection path. Then rerun the same diagnosis selection with the same filter,
limit, timeout, corpus, and trusted-library setup. Compare the reports:

```sh
cargo run -p psrs-cli -- diagnose --corpus passing --filter Functor --limit 20 \
  --out /tmp/psrs-after.json --timeout 20
cargo run -p psrs-cli -- diagnose --compare /tmp/psrs-before.json /tmp/psrs-after.json
```

The comparison reports per-case recovery, regression, stage/category changes,
input changes, and unmatched paths. Do not compare reports with incompatible
cohorts as if they were the same measurement. Investigate regressions and cases
whose inputs changed; record timeouts and crashes as incomplete evidence.

Compile diagnosis only establishes compile acceptance. If the behavior under
change reaches Wasm execution, add focused runtime checks that observe actual
values, call counts, output order, or traps as appropriate. A successful compile
does not establish correct runtime behavior. For runtime-gated driver tests
that support it, require Wasmtime explicitly:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib focused_runtime_test_name
```

Run the issue's required official-suite or scoreboard validation when the change
affects a gate or its acceptance evidence. Update D-04 and README measurements
only after rerunning the relevant scoreboard with the documented settings.
Small filtered diagnosis reports are useful for locating regressions; they are
not replacements for official acceptance measurements.

## 5. Report the evidence

For each iteration, report:

- the starting commit and whether the worktree already had changes;
- the exact baseline and comparison commands and selected cohort;
- compile outcomes by first-blocker stage/category, keeping exclusions,
  timeouts, crashes, and incomplete inputs separate;
- the reproducer, suspected semantic owner, and evidence for that hypothesis;
- focused tests, runtime behavior exercised, and required suite/scoreboard runs;
- what changed, what remains unresolved, and which validations were not run.

Do not turn a filtered cohort into a corpus-wide pass rate. Do not update a
scoreboard number from diagnostic output or from an unverified estimate.
