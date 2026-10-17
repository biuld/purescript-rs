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
| OR-06 | Immutable updates preserve unknown tails and type-changing field results, with exact evaluation order/count | Open-row updates lower through the owner's `Update` operation: base and update expressions lower exactly once in source order, replacement payloads erase through the common planner, and the runtime kernel patches existing labels on one fresh store while keeping the complete label/value sequence. Direct CC/MIR/GC runs replace a label and recover the rest, a missing label traps, forged operands fail the CC boundary, and three required-Wasmtime source runs cover source-order multi-label updates, a type-changing field over a retained tail, and a case-branch update. Official `ObjectUpdate.purs`, `ObjectUpdate2.purs`, `ObjectUpdater.purs`, `NestedRecordUpdate.purs`, `NestedRecordUpdateWildcards.purs` and derived-Functor `2616.purs` execute. Broader record library coverage is OR-09's scope. |
| OR-07 | Generic/recursive/imported uses execute without finite specialization as a correctness prerequisite | Generic and recursive source functions execute. Imported and remaining scope interactions are pending. |
| OR-08 | Source-typed runtime primitives preserve checked operand/result types and slot policy before ABI erasure | Runtime-owned contracts describe `length`, `key_at`, `payload_at` and `build`, including their projection to the one CC `RecordOperation` owner. P8 resolves bare or applied slot types to positive ordinary constructorless declaration evidence; P9 preserves and checks the provider export against the projected operation and rejects direct calls bypassing it. The driver module passes 6/6 with required Wasmtime (five executions including an imported applied-carrier slot, plus one unused invalid-signature rejection). |
| OR-09 | Library-owned searches and official unsafe wrappers; pinned source and JS oracle comparisons | Implemented for the four `Record.Unsafe` operations: the pinned package preserves the official wrappers and signatures, with algorithms in `PSRS.Record`; the package audit finds 50 packages/309 modules and no missing upstream module. Node conformance passes 13/13, and the independent Wasm oracle accepts the common values and three explicit trap cases. See the OR-09/OR-10 evidence below; full topic acceptance remains open. |
| OR-10 | Empty, Unicode, reserved-looking and absent keys; invalid unsafe-input target differences explicit | Partial: required-Wasmtime provider tests and the JS/Wasm oracle cover empty storage, private-use and supplementary Unicode labels, `__proto__` as an ordinary own key, absent/inherited get traps, and target divergence from JavaScript `undefined` and prototype lookup. Broader malformed-store, ABI and rejection-path combinations remain open. |
| OR-11 | CC and MIR reject malformed schema, plans, missing fields, mismatched endpoints and evidence | Eight CC-schema, six CC-plan and five MIR-owner/physical mutations reject. Six P9 provider-boundary rejections cover the checked registry (a valid base passes, while a second registered provider, a missing runtime binding, an export for another operation, a direct-call bypass and a noncanonical external signature reject); five direct build-input mutations trap. Checks for remaining operations are pending. |
| OR-12 | Canonical ABI and parameterized ADT fields use the same record storage policy | Erased record lists encode/validate both ABI directions, String and flags fields. Three filesystem programs execute; broader nested ABI execution remains required. |
| OR-13 | Value-sensitive required-Wasmtime programs, official TCOFloated/Deriving cases, and full scoreboards | Partial: the shared coverage/Partial follow-through remeasures all 365 roots at 287/340 with no regressions; `Let`, `ShadowedTCOLet`, and `Where` recover, while the earlier `TCOFloated`/`Deriving` successes remain. L5 reaches 81/97, with L1–L4 unchanged. Complete topic evidence still depends on the broader interaction, malformed-IR and ABI obligations above. |
| OR-14 | Formatting, full workspace tests, Clippy, final diff and design review | Formatting, source layout, full workspace and strict Clippy pass at the 2026-10-17 shared-coverage checkpoint: 2,093 passed / 0 failed / 5 ignored with required Wasmtime and the locked package, with all ignored scoreboard groups exercised separately. The slice diff/design review is complete; the final review against the complete topic remains open. |

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

Pattern-driven tail binding (`..rest`) remains unsupported at the surface.

## Open-row update slice: 2026-10-17 (later still the same day)

Open rows used to reject every record update with
`record representation has no canonical field labels`: the closed path names
canonical product indices, which a labelled store does not provide. The record
owner gains an `Update` operation (`cc/record/mod.rs`): CC lowering
(`cc/lower/record/open.rs`) lowers the base record once, then lowers every
update expression exactly once in source order, erasing each replacement
payload through the common planner into a private slot keyed by its literal
label; a duplicate label inside one update is rejected at its source position.
MIR lowering (`mir/lower/aggregate/record/operations.rs`) scans each
replacement label in the base's runtime sequence, traps when a label is absent
from the store, clones the value array once, patches the found slots, and
reuses the unchanged label sequence in a fresh header. The input store is
never written, the complete label/value sequence including the unknown tail is
retained, and closed-row updates keep their specialized product path.

Evidence:

- Direct CC/MIR/GC runs: `record_update_replaces_one_label_in_fresh_storage_and_keeps_the_rest`
  packs, updates and unpacks with the unchanged fields and the replaced value;
  `record_update_traps_on_a_label_missing_from_the_runtime_store` proves the
  honest trap; `forged_record_update_operands_fail_at_the_public_cc_boundary`
  rejects swapped and reused operands at the CC verifier.
- Required-Wasmtime source runs: source-order multi-label updates
  (`open_row_updates_replace_labels_in_source_order_and_keep_the_tail`), a
  type-changing field over a retained tail
  (`a_type_changing_open_row_update_keeps_a_string_in_its_unknown_tail`), and
  a case-branch update against the open-row scrutinee
  (`a_case_branch_updates_its_open_row_scrutinee`) all exit 42.
- The official roots `ObjectUpdate.purs`, `ObjectUpdate2.purs`,
  `ObjectUpdater.purs`, `NestedRecordUpdate.purs`,
  `NestedRecordUpdateWildcards.purs` and the derived-Functor `2616.purs`
  compile and finish correctly under the scoreboard conditions.
- The full debug workspace passes 2,062 tests with the required Wasmtime
  baseline; formatting, source layout and Clippy with warnings denied remain
  clean. Evidence: `/tmp/psrs-open-record-2026-10-17-update-workspace.log`.

The runtime board lifts to **282/340** (25 FFI groups excluded) on release
suite `8585a60b24a8a02eb8b5c132c596359422492e224f6199a133fefd2364476337`
(decomposition in
`/tmp/psrs-open-record-2026-10-17-runtime-281/summary.json`): exactly
`ObjectUpdate.purs`, `NestedRecordUpdateWildcards.purs` and `2616.purs` leave
the failing set. `RowPolyInstanceContext.purs` still fails to typecheck; its
undetermined-variable pair prints in either order across repeated runs of the
same binary, so that presentation order is not a stable result. Frontend
boards are unchanged; the slice changed only backend lowering and its tests.

OR-08 target bindings, OR-09 library record algorithms and the remaining
OR-11 rejection surfaces stay open.

At this update checkpoint, the standard-library package was `6fcef6cc`; its
official sources had not been rewritten, and `Record.Unsafe` foreigns still
reported missing target support. The subsequent OR-09 library work is tracked
separately. Do not close #124 or declare the open-record topic complete on
these paths.

## Checked record-storage primitive slice: 2026-10-17

`psrs-runtime` now owns source contracts for `psrs:record-storage#length`,
`key_at`, `payload_at` and `build`. The descriptor includes source parameter
and result types and the argument positions projected to the shared
`cc::RecordOperation` owner. P8 checks the checked Core source signature before
erasure, resolving bare and applied source types to a positively recorded
ordinary constructorless data carrier for `Slot`; missing, conflicting
ordinary/opaque/newtype evidence and constructor-bearing carriers reject. P9
retains provider identity through CC
and validates that each operation matches its checked export. Closed product
record arguments and results use the ordinary shared pack/unpack conversion.
These bindings lower to the existing record kernels through helper functions;
they do not create another physical record schema or codec. P9 checks the
preserved provider export, operation and physical signature; it trusts the P8
producer's checked source scheme and does not reconstruct `Slot` identity from
the erased arrays.

`build` copies both source arrays before constructing the store. It checks
nonnegative/equal lengths, recovers each label from the source array's erased
element storage, rejects null labels and slots, enforces strictly increasing
Unicode-scalar label order, and returns a fresh `(labels, values)` store. The
source wrapper still owns searches and mutation algorithms.

Evidence:

- Final-tree, required-Wasmtime source tests in
  `psrs-driver::tests::record_storage` pass 6/6 with
  `PSRS_REQUIRE_WASMTIME=1`: five execute generated record operations
  (including an imported applied-carrier `Cell Int` slot), and
  `record_storage_validates_unused_source_contracts_before_erasure` rejects
  unused bad source signatures at P8. The executions cover imported renamed ordinary `Cell`,
  higher-order length, key and payload reads, nested and callable slots,
  mutation of both original arrays after `build`, closed-record signatures,
  empty storage, Unicode and prefix ordering, duplicate/unsorted labels,
  unequal input lengths, and negative/out-of-range reads.
- Direct GC/MIR tests in
  `mir::gc_tests::record::build` pass 2/2 direct executions with required
  Wasmtime. The checked provider path builds through the shared owner; two
  malformed MIR executions inject a null into each input array and verify the
  label cast and payload checks trap.
- Backend source-contract tests prove valid ordinary carrier evidence and
  reject missing metadata, ordinary/opaque and ordinary/newtype conflicts, and
  constructor-bearing data. CC rejects providerless `Build` evidence, the
  shared projection test covers each export/operation pair and rejects a
  mismatched provider operation, and six P9 provider-boundary rejections cover
  the provider registry: a valid base passes both checked stages, while a
  second registered provider, a missing runtime binding, an export for another
  operation, a direct-call bypass and a noncanonical external signature all
  reject. All of this is final-tree evidence in the 2,084-test workspace run.
- An independent three-module imported fixture passed the library runner and
  exited 42 after mutating its original labels and payload arrays. The
  comparable diagnosis cohort is `/tmp/psrs-open-record-or08-independent-final.json`
  and `/tmp/psrs-open-record-or08-independent-compare.json`; runtime output is
  `/tmp/psrs-open-record-or08-independent-run-fixed/run.json`.

The checked primitive contract and MIR slice is implemented. OR-09 library
wrappers and algorithms, remaining OR-11 rejection surfaces, and complete
integration checks remain open; this does not close the open-record topic.

## Library record algorithms and unsafe wrappers: 2026-10-17

Package `e556bf2f8cb377c049d2ff0a5a97b484c7b36bc5` supplies the OR-09
implementation at fingerprint `fnv1a64-v1:7d96f1c44d8d52cc`. Its clean
checkout is `/tmp/psrs-open-record-stdlib-or09`; the compiler lock retains
`../psrs-stdlib`, so use
`PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09` for local compiler
commands. `Record.Unsafe` keeps the four official exports and signatures as
ordinary wrappers; `PSRS.Record` owns the search, insertion, deletion and
replacement algorithms and calls the checked `psrs:record-storage` operations.

The final vendoring audit covers 50 packages and 309 modules, with no upstream
modules absent from the vendor (`/tmp/psrs-open-record-or09-evidence/final-audit/`).
The library's Node conformance suite passes 13/13. Its independent target
oracle report is
`/tmp/psrs-open-record-or09-evidence/oracle-edge-final/record-unsafe.json`:
the common value behavior is accepted; empty-record, missing-key and
inherited-key get cases produce the expected target traps; checks for updates,
deletes, callbacks, nested arrays and Unicode labels pass. The explicit target
differences are that Wasm has no JavaScript `undefined` result or prototype
lookup and treats `__proto__` as an ordinary own key. The report binds this
evidence to the package fingerprint and CLI SHA-256
`d91a5b9943b871e4a90997ce0f0fde2123336aa248a7ca12272cd690425dceac`.

This covers OR-09 and part of OR-10, not all unsafe-input and integration
surfaces. Broader malformed-store and ABI interactions, remaining OR-11
rejection operations, OR-07/OR-12 combinations, and OR-13's full value-sensitive
boards remain open. Keep #124 In Progress until all required slices and final
boards are verified.

## Final validation and board: 2026-10-17

The final checkout state carries the checked record primitives, the new
package, all source-layout repairs and the applied-carrier/P9 tests together.
Validation at this tree: formatting and source layout pass, Clippy passes with
warnings denied, and the release suite binary rebuilds with SHA-256
`37bcdd007466f2ed91f2cbd06962c584bcdd00caef10f73465a5070f36d1f4ee`. The full
debug workspace passes 2,084 tests, failing zero, ignoring five, with
`PSRS_REQUIRE_WASMTIME=1` and `PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09`
(`/tmp/psrs-or08-final-tree-workspace.log`).

The runtime board was re-measured over all 365 program roots with the same
per-root worker pattern: **284/340** with all exits zero and 25 FFI groups
excluded (per-program rows in
`/tmp/psrs-open-record-or08-or09-runtime/summary.json`). Exactly
`Deriving.purs` and `TCOFloated.purs` leave the failing set — the two
`Record.Unsafe.unsafeGet` blockers — and no other failure changed state. The
56 non-agreeing roots split as 31 P5 typecheck, 11 P5 kind check, 5 P8
closure/linking (3 closure capture, 1 recursive local binding, and the
`Test.Assert.checkThrows`-blocked `TCOMutRec.purs`), 5 P3 resolve, 2 P0 lex
(DEC-16), one 10-second runtime deadline, and the bounded BigFunction
compilation, which reached the measurement cap near the 2 GiB guard before a
diagnostic. `Test.Assert.checkThrows` remains the one record-adjacent foreign
without a target contract. The topic is not closed; #124 stays In Progress.

## Shared coverage / Partial follow-through: 2026-10-17

The [handoff's final resumption](open-records-handoff-2026-10-17.md#partial-exhaustiveness-follow-through)
records completion of the blocked checker integration. Type checking and CC now
consume one usefulness engine. Nested pattern types are resolved after branch
unification; inferred and declared `Partial` constraints share the existing
elaboration convention; annotated non-exhaustive cases report at P5 with a
witness. Transparent newtype cycles such as official `Data.Void` retain erased
storage without recursive layout expansion. This work edits no package source.

The final release runtime board is 287/340, up from 284/340 without regressions.
`Let`, `ShadowedTCOLet`, and `Where` now execute. L5 is 81/97 (`NoInstanceFound`
52/53), up from 77/97 (48/53), and L1–L4 stay unchanged. The full measurement,
resource-bound outcome, and artifact identity are recorded in
[the program scoreboard](../runtime-program-scoreboard.md#shared-coverage-and-partial-recheck-2026-10-17).
This is progress on the topic; OR-07 and OR-10–OR-14 retain their remaining
obligations rather than inheriting completion from the compiler-wide board.

Final workspace validation for this follow-through uses the locked package via
`PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09` and required Wasmtime:
**2,093 passed / 0 failed / 5 ignored**, with all ignored scoreboard groups
exercised separately. Formatting, source layout and strict full-workspace
Clippy pass. Evidence: `/tmp/psrs-partial-workspace-final.log` and
`/tmp/psrs-partial-clippy-accepted.log`. The final diff and design review for
this slice are complete; that does not close the broader topic obligations.
