# Open Record Implementation Acceptance

**Design:** [Open record rows](../../design/backend/fp/open-records.md).
**Status:** In progress. Read/transport evidence is recorded below; the topic is not complete.

All design requirements apply, including adjacent erased and canonical-ABI
boundaries. Complete each required vertical slice before declaring this topic
complete. Source, direct IR, malformed IR and executed runtime evidence remain
separate. Missing Wasmtime is a failure.

| ID | Requirement | Evidence / status |
| --- | --- | --- |
| OR-01 | One checked immutable storage schema; reject stale/wrong references, nullability and element kinds | CC schema tests pass, including eight malformed mutations. MIR owner/physical-schema tests reject five mutations. |
| OR-02 | Open rows retain full storage independently of prefix and instantiation | Shared-layout test and source accessor/identity runs pass across distinct rows and field indices. Updates are tracked in OR-06. |
| OR-03 | Closed-to-open pack and open-to-closed recovery with complete label validation | Source return recovery and direct CC/MIR round trips execute; wrong labels and payload lengths trap. |
| OR-04 | The same store owns bare erased record transport, recursively through arrays, records, ADTs and callable adapters | Eight source runtime tests cover Array/ADT erased slots, independent nested rows, callable fields and callable results. Broader interaction coverage remains required. |
| OR-05 | Known fields and record patterns select by label, including differing prefixes and canonical indices | Known-field selection executes through shared find/read kernels. Open-row record patterns project by label through the same kernel: a realize unit test proves no product projection against the store, and two required-Wasmtime source runs exercise differing prefixes plus nested open-row values. `FieldPuns.purs`, `FieldConsPuns.purs` and `ReservedWords.purs` additionally execute in the official corpus. Pattern-driven wildcards/tail binding checks for remaining scope interactions. |
| OR-06 | Immutable updates preserve unknown tails and type-changing field results, with exact evaluation order/count | Pending |
| OR-07 | Generic/recursive/imported uses execute without finite specialization as a correctness prerequisite | Generic and recursive source functions execute. Imported and remaining scope interactions are pending. |
| OR-08 | Source-typed runtime primitives preserve checked operand/result types and slot policy before ABI erasure | CC structural reads have shared operand/result shapes and isolated MIR helpers. Target library bindings are pending. |
| OR-09 | Library-owned searches and official unsafe wrappers; pinned source and JS oracle comparisons | Pending |
| OR-10 | Empty, Unicode, reserved-looking and absent keys; invalid unsafe-input target differences explicit | Pending |
| OR-11 | CC and MIR reject malformed schema, plans, missing fields, mismatched endpoints and evidence | Eight CC-schema, six CC-plan and five MIR-owner/physical mutations reject. Checks for remaining operations are pending. |
| OR-12 | Canonical ABI and parameterized ADT fields use the same record storage policy | Erased record lists encode/validate both ABI directions, String and flags fields. Three filesystem programs execute; broader nested ABI execution remains required. |
| OR-13 | Value-sensitive required-Wasmtime programs, official TCOFloated/Deriving cases, and full scoreboards | Pending |
| OR-14 | Formatting, full workspace tests, Clippy, final diff and design review | Formatting, source layout, full workspace (2,053 passed, 0 failed) and Clippy pass at the 2026-10-17 post-repair checkpoint. The final diff and design review against the complete topic remain open. |

## Starting evidence

On 2026-10-17, compiler base `fe6dd78f` plus the command, scoreboard and
contextual-application changes (committed later the same day), package `6fcef6cc` /
`fnv1a64-v1:5fcbb57525c63eb1`:

- Direct `unsafeGet "value" { value: 42, label: "x" }` fails at P8 library
  linking because `Record.Unsafe.unsafeGet` has no target implementation.
- Ordinary `readValue :: forall r. { value :: Int | r } -> Int` fails at P8
  closure conversion because open rows have no runtime layout.
- The official `Ord` instance on `{ value :: Int }` reaches the same missing
  `unsafeGet` binding. This is a real dependency of `TCOFloated.purs`.

Trace diagnoses are `/tmp/psrs-record-boundary/{Direct,Open,Ord}.json`, with
matching sources and logs. They distinguish the foreign binding gap from the
open-row ABI gap. The pre-workspace accepted baseline was 2,037 tests and
273/340 programs; the post-repair checkpoint above holds
2,053 tests and 276/340 programs. Those numbers do not establish acceptance of
this topic.

## Read/transport integration evidence

The source tests are `psrs-driver`'s `tests::records::open_rows`: eight required
Wasmtime runs, all exiting 42. The original simple accessor also passes through
the library-owned executable runner at
`/tmp/psrs-record-boundary/Open-run/run.json`. The initial Array/ADT tests exposed
cast failures while erased records still used products. Switching all bare
record payloads to the same labelled store fixes both; those failures are
retained in `/tmp/psrs-open-record-source-tests.log`.

Direct IR tests cover a String preceding nested ArrayMap control flow. Physical
non-null fields, including String, now use one shared persistence/restore
operation across nested conversion labels. Record operations and ABI aggregate
conversions use shared generated helpers so their internal loops do not change
the caller's live-reference scope.

The first canonical-ABI integration run rejected filesystem record lists. The
ABI now retains the original logical product and the owner's physical store as
one view, and nested list copies retain complete Field descriptors rather than
only erased shapes. Three filesystem programs pass again with actual Wasmtime;
record/string/flags list fixtures validate load and store encodings. The new MIR
checks tie that view to its registered schema and physical GC definitions.

Evidence logs include `/tmp/psrs-record-composition-tests.log`,
`/tmp/psrs-record-checkpoint-tests.log`, `/tmp/psrs-record-abi-storage-tests.log`
and `/tmp/psrs-record-filesystem-owner-verified.log`. The initial workspace run
stopped at source-layout rules, not a backend test failure (518 backend tests
passed). Module roots were moved into directories with `mod.rs`; the focused
source-layout check passes.

## Post-repair checkpoint, 2026-10-17

The machine control pair previously published a private two-field product where
the common erased-record protocol required one. The final repair plans one
conversion in each direction from the common payload planner
(`cc/lower/control/pair.rs`), checks the complete expected plan at its exact
ordered location (`plan/boundary/{ops,primitives}.rs`), and keeps a general
helper-selection rule: a fixed `RecordPack` over all-`Identity` fields needs no
helper because it emits only projections and allocations. No machine pair
special case owns the rule.

That checkpoint passes every gate:

- With `PSRS_REQUIRE_WASMTIME=1`: all six `tests::control_costs` tests, all 32
  `tests::control_regressions` tests, and all 29 `tests::records` tests
  (including the open-row runs and the updated
  `typechecks_an_open_record_row_by_label`, which now requires its honest exit
  1 while `setX` is unsupported).
- All **518 backend tests** and the full workspace: **2,053 passed, 0 failed,
  5 ignored** scoreboard tests. Formatting, the source-layout check and Clippy
  with warnings denied all pass. Evidence:
  `/tmp/psrs-open-record-2026-10-17-workspace.log`.
- The frontend boards are unchanged at this tree: L1 904/908 (58 excluded),
  L2 72/72 and passing resolution 403/413, L3 39/48, L4 39/50, L5 77/97.
  Evidence: `/tmp/psrs-open-record-2026-10-17-boards.log`.
- The runtime board remeasured **276/340** (25 FFI groups excluded), the
  per-program decomposition in
  `/tmp/psrs-open-record-2026-10-17-runtime-checkpoint-276/summary.json`.
  Relative to the 273 baseline, `1807.purs`, `NestedRecordUpdate.purs` and
  `ObjectGetter.purs` newly pass; every failing reason otherwise matches the
  pre-repair measurement. The release suite binary is
  `61d4dea7488511191630dcf2adb060f2de9d62701786893434b6b6f955212b7e`, on
  compiler `fe6dd78f` plus the changes this handoff covers, committed
  the same day, with
  package `6fcef6cc` / `fnv1a64-v1:5fcbb57525c63eb1` and Wasmtime 49.0.3.

This satisfies OR-14's status condition at a read/transport checkpoint, not the
topic's completion condition: OR-05 record patterns, OR-06 updates, OR-08
target bindings, OR-09–OR-10, remaining OR-11 rejections, broader OR-12/OR-07
interactions, and OR-13 official cases stay open, all recorded in the table
above. D-04 promotes the 276/340 row to the gate table.

## Record-pattern slice: 2026-10-17 (later the same day)

Open rows used to fail record patterns with an
`unsupported aggregate conversion` because the decision-DAG projection treated
the shared labelled store like a closed product. Record patterns now project
through one shared `project_labelled_field` kernel with the accessor path
(`cc/lower/record/open.rs`): a literal label, `Find`, `Read`, then the common
payload recovery to the requested use type. The pattern compiler still
specializes closed records into their checked products — only labelled stores
are read by label. The kernel rejects a pattern whose claimed representation is
not its labelled store, and never assumes a canonical index for unknown labels.

Evidence:

- The realize unit test
  `open_record_patterns_find_their_field_label_in_the_labelled_store` proves a
  case against an open row produces `Find`/`Read` assignments, no `ProductGet`,
  and verifies as CC.
- Required-Wasmtime source runs
  `a_record_pattern_selects_labels_across_distinct_open_rows` and
  `nested_record_patterns_select_open_row_values` both exit 42 across differing
  prefixes and nested open-row values.
- The official corpus cases `FieldPuns.purs`, `FieldConsPuns.purs` and
  `ReservedWords.purs` — previously blocked on the closed-conversion path —
  compile and finish with `Done` and exit zero.
- The full debug workspace passes 2,056 tests with `PSRS_REQUIRE_WASMTIME=1`;
  formatting, source layout and Clippy with warnings denied remain clean.
  Evidence: `/tmp/psrs-open-record-2026-10-17-pattern-workspace.log`.

The runtime board was re-measured **279/340** (25 FFI groups excluded) with
release suite `7c9f8839dbd7cf73d63322e90ca2eefd5a904551c63cf73b76bb77b64673e2eb`
(results in `/tmp/psrs-open-record-2026-10-17-runtime/summary.json`; the 276
control-repair checkpoint copy is
`/tmp/psrs-open-record-2026-10-17-runtime-checkpoint-276/`). Exactly the three
pun-blocking roots newly pass and no failure changed reason.

Updates stay OR-06's open slice, and pattern-driven tail binding
(`..rest`) remains unsupported at the surface.

The standard-library package remains `6fcef6cc`; its official source has not
been rewritten, and `Record.Unsafe` foreigns still report missing target support.
Do not close #124 or declare the open-record topic complete on these read paths.
