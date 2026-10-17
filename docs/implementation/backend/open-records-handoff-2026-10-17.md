# Open Records and Roadmap Handoff — 2026-10-17

**Latest handoff update: 2026-10-18.** This opening snapshot supersedes every
historical clean/dirty checkout statement below. The implementation and its
matching tests/design/acceptance documents are committed on local `master`:

1. `e5972241` — Share pattern coverage between type checking and closure conversion.
2. `e331f9ba` — Lower generic local rows without mandatory specialization.

The implementation checkpoint is `e331f9ba9a785f24496d37d0a1b0d92e8a1c0ca6`.
The handoff update is the documentation-only commit immediately following that
checkpoint; use `git log -3 --oneline` to identify the final handoff HEAD. After
that commit the compiler checkout is clean, with no uncommitted implementation
or test changes. Nothing was pushed; no PR, issue or project-board action was
performed. Implementation stops here for the receiving agent.

**Acceptance status:** The open-record topic remains **In Progress**. Core
storage, operations, generic closure execution, library wrappers and nested ABI
have execution evidence; the complete requirement-by-requirement integrated
acceptance review is still outstanding. Do not describe this as completed open
records or infer topic completion from the compiler-wide runtime count.

## Receiving agent start point

1. Inspect `git status --short` and `git log -3 --oneline`; preserve any newer
   changes made after this snapshot. Read the normative
   [open-record design](../../design/backend/fp/open-records.md),
   [DEC-22](../../decision/DEC-22-labelled-record-storage.md) and
   [OR-01–OR-14 checklist](open-records.md).
2. Audit OR-01–OR-12 as one contract, mapping each requirement to its actual
   producer, verifier, direct-IR tests and executed runtime evidence. Resolve
   OR-04/OR-05's remaining combination-coverage statements: inspect nested
   aggregate/callable transport and record-pattern scope interactions, add
   minimal regressions for concrete gaps, and repair their common owner if
   they fail. Do not replace those statements with an unsubstantiated Done.
3. OR-07's forced-specialization prerequisite has been removed. Preserve its
   unoptimized-Core single-reader assertion and negative-control evidence.
   OR-10/OR-11 now cover each read/update operation; OR-12 checks actual nested
   ABI values in both directions. Their detailed evidence is in the final
   "Generic local rows and nested ABI follow-through" section below.
4. Complete OR-13/OR-14 only after that integrated audit, relevant gap fixes and
   current-tree validation. Keep design/code/tests together by topic, remeasure
   affected scoreboards, and update D-04 from measured results. No current
   focused test failure is known. BigFunction's resource bound and the missing
   `Test.Assert.checkThrows` target behavior remain explicit compiler-wide
   limitations, without fabricated foreign implementations.

## Current package and validation snapshot

- Compiler repository: `/Users/biu/Projects/purescript-rs`, local `master`,
  implementation checkpoint `e331f9ba` plus the following handoff-only commit.
- Development package: `/tmp/psrs-open-record-stdlib-or09`, clean at
  `e556bf2f8cb377c049d2ff0a5a97b484c7b36bc5`, source fingerprint
  `fnv1a64-v1:7d96f1c44d8d52cc`. Its local commit has not been pushed.
- The compiler lock retains `../psrs-stdlib` with that revision. Set
  `PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09` for package-consuming
  commands. The shared sibling `/Users/biu/Projects/psrs-stdlib` is separately
  clean at `1e01310daa7e56de5ed379029d6a24dcf39e879c`; it was not modified.
- The implementation tree now committed above passed full workspace tests:
  **2,100 passed / 0 failed / 5 ignored**, with required Wasmtime 49.0.3 and
  the selected development package (`/tmp/psrs-or07-workspace.log`). Formatting,
  source layout, strict workspace Clippy and diff whitespace checks pass
  (`/tmp/psrs-or07-fmt.log`, `/tmp/psrs-or07-clippy.log`). Committing and editing
  this handoff did not change tested source; the long checks were not rerun
  solely for commit metadata or this documentation update.
- All five ignored scoreboard groups were exercised separately. Frontend:
  L1 904/908, L2 72/72 (passing resolution 403/413), L3 39/48, L4 39/50,
  L5 81/97 (`NoInstanceFound` 52/53), `/tmp/psrs-or07-frontboards.log`.
- Runtime: **287/340**, unchanged success set, 365 roots, 74 support inputs,
  25 FFI groups excluded. The 53 non-agreeing roots split into 31 P5 typecheck,
  11 P5 kind check, 2 P8, 5 P3 resolve, 2 P0 lex, one runtime timeout and
  BigFunction. BigFunction stops at the 2 GiB RSS guard in 27.89 seconds with
  observed peak 2,141,824 KiB; no compile acceptance or performance repair is
  claimed. Evidence: `/tmp/psrs-or07-runtime/summary.json`, `results.jsonl` and
  `big-function-checked.json`.
- Release suite: `/tmp/psrs-or07-suite`, SHA-256
  `52a9b77e706fd2030ed8c235c75f3a313ee118a18de0b49e4be21eb375e03df1`.
- Library-owned Node conformance passes 13/13
  (`/tmp/psrs-or07-node-conformance.log`). The current debug CLI's independent
  `Record.Unsafe` JS/Wasm oracle accepts the value program and all three
  expected-trap programs (`/tmp/psrs-or07-record-oracle/record-unsafe.json`).
  The package remains unchanged. These `/tmp` artifacts are local evidence;
  regenerate them if they are no longer available to the receiving agent.

## Historical implementation and checkpoint notes

The sections below retain the earlier investigation and exact evidence. Their
branch, HEAD, clean/dirty and validation statements describe their own snapshots;
the opening 2026-10-18 snapshot is authoritative for resumption.

## Committed state (2026-10-17, evening)

The work was committed in five topical commits on `master`, in order:

1. `f712b8e7` Run the configured library runner and measure programs as groups
2. `b9148f59` Check rank-N arguments against a monomorphic contextual result
3. `3c31db7e` Give open rows one immutable labelled record store
4. `48704baf` Select open-row record patterns by label
5. `b7778658` Update open rows through one immutable labelled store

`stdlib.lock.json` pins the same revision
`6fcef6ccbab1f3459af2573bbef4ae25b86852d4` as at the pause; the shared sibling
`psrs-stdlib` checkout remains at its earlier unrelated state, unchanged.

## Checkout and package ownership

- Compiler: `/Users/biu/Projects/purescript-rs`, branch `master`, current HEAD
  `b7778658` (at the original pause it was
  `fe6dd78f14d5f4913e78d2d30c57e3b38cc5c900`).
- The compiler work this document describes was uncommitted at the pause
  (including new files and
  module-directory moves) and is preserved in the committed state above.
- Use `PSRS_STDLIB_ROOT=/tmp/psrs-symbol-reification-stdlib` for every compiler
  command consuming the package. This isolated package is clean at
  `6fcef6ccbab1f3459af2573bbef4ae25b86852d4`, fingerprint
  `fnv1a64-v1:5fcbb57525c63eb1`; `stdlib.lock.json` pins that revision.
- The shared sibling `/Users/biu/Projects/psrs-stdlib` is instead at
  `1e01310daa7e56de5ed379029d6a24dcf39e879c`. Preserve it; do not switch it or
  change the lock merely to make validation run. Its state was changed outside
  this work.
- The isolated package has local command-runner and Symbol adapter commits;
  they have not been pushed. Preserve the package directory as part of handoff.

A local recovery snapshot is in
`/tmp/psrs-open-record-handoff-2026-10-17/`: `tracked.patch`,
`untracked.tar.gz`, `status.txt`, and `source-sha256.json`. It was captured
before adding this handoff document. These are recovery artifacts, not a
request to reset or reapply changes in the already populated checkout.

## Earlier slices already validated

These changes share the dirty tree and must not be discarded while repairing
open records:

1. The runtime scoreboard groups each program root with its support directory,
   following reference `purs`. The inventory is 439 sources: 365 roots plus 74
   support inputs. Twenty-five FFI groups are excluded, leaving 340 programs.
2. Configured executable normalization uses checked polymorphic instantiation
   through Core's common `Module::checked_instantiation`. The library-owned
   `PSRS.Command.run :: forall a. Effect a -> Int` runs the action exactly once
   and discards its result. Unchanged official `Sequence.purs` prints `Done`
   and exits zero. See `polymorphic-command-runner.md` in this directory.
3. Symbol reification preserves official pure `Data.Symbol` definitions and
   changes the private target foreign boundary to a typed `Unsafe.Coerce`
   delegate. Package conformance compares actual `purs`/Node results, callback
   behavior, strictness, strings and rejection cases.
4. Contextual application checking propagates a monomorphic expected result
   before checking a rank-N argument. Polymorphic/constrained contexts retain
   synthesis and subsumption; the common skolem guard remains intact. Result
   dictionaries survive `instantiate_use`/`apply_constraints`. See
   `../frontend/contextual-application-results.md`.

The earlier integrated workspace run passed **2,037 tests**, zero failures,
five ignored scoreboard tests; required scoreboards were run separately.
Evidence: `/tmp/psrs-symbol-context-workspace-final.log`. The earlier runtime
snapshot was **273/340**, all successful cases exiting zero, with 67 remaining.
Its manifest and decomposition are
`/tmp/psrs-symbol-runtime-acceptance/summary.json`. This is a baseline, not
evidence that the subsequent open-record changes are green.

## Current architecture and ownership

Read these together before changing the representation:

- Design: `docs/design/backend/fp/open-records.md` (Draft).
- Decision: `docs/decision/DEC-22-labelled-record-storage.md` (Proposed).
- Checklist: `docs/implementation/backend/open-records.md`, OR-01–OR-14.
- Existing normative representation, erasure and control contracts.

Closed typed records retain specialized products. Open rows and all bare erased
record values use one immutable labelled store: a product header containing
parallel `Array String` keys and `Array Erased` payloads. Keys are sorted and
unique; producers establish runtime contents. Schema verification proves the
physical schema, not arbitrary array contents. Complete closed recovery checks
lengths and every expected key before recovering fields. No source type IDs or
module-name tests become runtime record identities.

Implementation map:

| Owner | Files and responsibilities |
| --- | --- |
| CC schema | `cc/record/`: immutable `RecordStorage`, schema validation, structural `RecordOperation` operand/result shapes |
| Layout | `cc/layout/{aggregate,scalar,protocols}.rs`: shared open-row header; closed products; canonical erased arrays |
| Payload | `cc/payload.rs`: recursive common pack/unpack, array and callable transport; `stored_guest_layout` retains original logical fields |
| Conversion | `cc/convert.rs`, `cc/lower/conversion/record.rs`: explicit `RecordPack`/`RecordUnpack`, closed/open boundaries |
| Source access | `cc/lower/record/open.rs`: find known field by literal label, read erased slot, recover through common payload policy |
| CC rejection | `cc/verify/ops/{record,products}.rs`, `aggregate/record.rs`: schema, endpoints, labels, nested conversion checks |
| MIR | `mir/lower/aggregate/{fields.rs,record/}`: physical-reference persistence, pack/unpack, checked reads; common ProductMap field transforms |
| Helper ownership | `mir/lower/conversion_helpers/`, `mir/lower/record_call.rs`: deduplicated conversion/read helpers and symbol allocation |
| ABI | `GuestLayout::RecordStorage`, `mir/wit/`, Wasm list `record/storage/`: retain both logical record and physical store, recurse through full Field descriptors |
| MIR rejection | `mir/verify/records.rs`: registered owner, GC array/header definitions, logical fields/labels and root hints |

`cc/assignment.rs` was extracted from `cc/mod.rs` to respect the 500-line rule.
The old single-file conversion helper, Wasm list record and driver record tests
were moved into directories with `mod.rs`; deletions in git status are these
moves, not removals of coverage. Source-layout rules require that grouping.

## Validation before the last repair

- All **518 backend tests** passed in the failing workspace run; the full
  workspace did not pass. Focused malformed-IR coverage includes eight schema,
  six conversion-plan and five MIR owner/physical mutations.
- Eight new required-Wasmtime source tests in `tests::records::open_rows`
  exited 42: different row indices, recursive access, complete closed recovery,
  erased ADT and Array slots, independent nested rows, callable fields and
  callable closed-record results converted to open rows.
- Direct IR round trip with a String preceding nested ArrayMap exits 85.
  Incorrect keys and short payload arrays trap in actual Wasmtime execution.
- Record/string/flags list fixtures encode and validate both ABI directions.
  Three filesystem programs execute again after ABI views retained their
  complete logical field descriptors.
- Rank-N tests, source-layout check, formatting and Clippy passed before the
  last control repair. The current tree must be rechecked after that repair.
- Public Symbol conformance rerun with the pre-repair debug CLI was accepted:
  `/tmp/psrs-record-read-symbol-oracle/symbol.json`.

Useful focused evidence:
`/tmp/psrs-record-composition-tests.log`,
`/tmp/psrs-record-checkpoint-tests.log`,
`/tmp/psrs-record-abi-storage-tests.log`,
`/tmp/psrs-record-filesystem-owner-verified.log`,
`/tmp/psrs-open-record-rank-n.log`,
`/tmp/psrs-record-module-layout.log`,
`/tmp/psrs-record-read-clippy-final.log`.

## Exact full-workspace failure and last repair

`/tmp/psrs-record-read-workspace-final.log` finished with exit 101. The driver
library section had **786 passed, 7 failed**. The seven failures were:

1. `tests::control_costs::control_ancestry_probe_observes_independent_non_tail_extents`
2. `tests::control_costs::control_tail_handlers_bound_extent_ancestry_and_post_gc_live_heap`
3. `tests::control_costs::replay::queue_shape_changes_traversal_with_equal_callbacks_requests_and_ancestry`
4. `tests::control_costs::replay::saved_callback_length_changes_actual_loop_work_at_fixed_requests_and_ancestry`
5. `tests::control_costs::replay_invocations_grow_with_requests_at_fixed_ancestry`
6. `tests::control_regressions::root::sequential_and_exclusive_runs_preserve_independent_invocations_after_optimization`
7. `tests::records::typechecks_an_open_record_row_by_label`

The first six were actual Wasmtime **cast failures**, not changed cost
thresholds. Do not weaken runtime or measurement assertions. The last test
still expected open-row compilation to be unsupported even though its reachable
accessor now executes. It was changed to run and require exit 1; that changed
test has not yet been rerun.

The last bounded repair bridges the machine's private two-field result product
and the new public erased record protocol:

- `cc/lower/control/pair.rs` obtains exact erase/recover plans from the common
  payload planner; no separate record codec is introduced.
- `Machine::pair` packs before publishing an erased result;
  `Machine::pair_field` recovers the canonical store before projecting the
  private product. The machine pair representation itself stays a product.
- `plan/boundary/{ops,primitives}.rs` checks the additional conversion at its
  exact ordered location and checks the complete expected plan. It does not
  silently strip arbitrary conversions as aliases.
- `cc/layout/control/mod.rs` ensures the record schema exists for machine
  pairs, including a machine without live source records. Its array-layout
  unit expectation was changed from two arrays to three, reflecting labels.
- Fixed `RecordPack` whose fields are all `Identity` now emits straight-line
  code instead of a conversion helper. This applies generally, not by control
  symbol. It avoids introducing a helper tail target into a machine terminal
  whose exact targets are checked by MIR. Packing with actual field conversions
  and unpacking still use helpers to isolate internal control flow.

An intermediate attempt failed the CC construction check because the producer
and checker edits were incomplete; that was corrected. The next attempt was
rejected by MIR's exact terminal-target check after helper tail-call rewriting;
the straight-line pack rule above addressed that case. The final focused run
**passed** the independent-invocation regression (both source forms):
`/tmp/psrs-record-control-pair-fix.log`, one passed, 792 filtered out.

This is the **only confirmed post-repair runtime result**. The five cost tests,
changed old record test, backend unit expectation, full workspace, Clippy and
scoreboards have not been rerun after the repair. Review the general helper
selection rule and both construction/verifier boundaries before accepting it.
No temporary checker debug printing remains.

## Fresh scoreboard snapshot, before the control repair

The two release cohorts finished successfully as scoreboard measurements:

- `/tmp/psrs-record-read-runtime-0.log`: 137/170, 13 excluded.
- `/tmp/psrs-record-read-runtime-1.log`: 139/169, 12 excluded.
- `BigFunction.purs` was separately stopped above 2 GiB RSS: peak 2,101,360 KiB,
  22.27 seconds, signal 9, before a compiler diagnostic.

Combined: **276/340**, 25 FFI groups excluded, 64 remaining. Relative to the
273 baseline, `1807.purs`, `NestedRecordUpdate.purs` and `ObjectGetter.purs`
newly pass, with no newly failing program roots in these snapshots. Remaining:
31 typecheck, 11 kind check, 10 closure conversion, 5 resolve, 2 lex (DEC-16),
3 foreign-library linking, one 10-second runtime timeout, one compiler resource
bound. `Deriving` and `TCOFloated` still need `Record.Unsafe.unsafeGet`;
`TCOMutRec` still reaches unsupported `Test.Assert.checkThrows`.

The release suite binary SHA256 was
`927caf748a852d6cff0b888700c876ba77986ae9cf9719cff124275e1dd578ae`.
The package was `6fcef6cc`. A parsed comparison is saved as
`/tmp/psrs-open-record-handoff-2026-10-17/runtime-pre-pair-fix-summary.json`.
It deliberately labels this a pre-repair measurement. Do not reuse its binary
as evidence for current source.

`/tmp/psrs-record-read-frontend-boards.log` records four passing board test
invocations: parse agreement includes 410/413 passing files with 26 exclusions;
resolution 72/72 failing and 403/413 passing; kind 39/48; type 39/50;
row/typeclass 77/97. The earlier full parse total was 904/908; inspect the
new log's complete decomposition before updating a total.

D-04 and README still state the **273/340 earlier snapshot**. They have not
been promoted to this new checkpoint because its full regression was red and
the final repair needs validation. Update measurements with exact code/package
identity after rerunning the relevant gates; do not call 276 full acceptance.

## First actions for the next agent

1. Inspect current status and this handoff; preserve all dirty work and use the
   isolated package. Review the control pair repair against the common erasure
   and checked-plan contracts.
2. Run the five cost tests, existing control regressions and the changed old
   open-row test with required Wasmtime. Diagnose any remaining failures before
   extending the feature. Do not broaden casts or relax assertions to go green.
3. Run backend tests (including the updated control array owner expectation),
   source-layout checks, full workspace, formatting and Clippy. Only then
   describe this read/transport checkpoint as integrated.
4. Rebuild and rerun affected scoreboards for the final source, maintain exact
   measurement identity, and update D-04/checklist/README proportionally.
5. Continue the outstanding open-record vertical slices below. Keep the topic
   explicitly incomplete until all required execution and rejection evidence
   exists.

Representative commands, run from the compiler checkout:

```sh
export PSRS_STDLIB_ROOT=/tmp/psrs-symbol-reification-stdlib
export PSRS_REQUIRE_WASMTIME=1
cargo test -p psrs-driver --lib tests::control_costs
cargo test -p psrs-driver --lib tests::control_regressions
cargo test -p psrs-driver --lib tests::records
cargo test -p psrs-backend
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
PSRS_ORACLE=annotations cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

Consult the current harness filters and resource guard before launching a full
runtime board; `BigFunction` is a known memory risk, not a free compiler pass.
The historical timeout case must remain a separately reported non-agreement.

## Required work still open

- Open-row record patterns: use label projection, not product-index guesses.
- Immutable and type-changing updates: preserve unknown tails, evaluate base
  and update expressions exactly once in source order. Existing pattern and
  update paths still assume product labels in places.
- Runtime coverage of structural Length/LabelAt and invalid read boundaries.
- Checked small runtime storage bindings and library-owned public
  `Record.Unsafe` get/has/set/delete wrappers; official source fidelity and
  actual upstream JavaScript differential conformance.
- Empty, Unicode, reserved-looking and missing keys; explicit target semantics
  for JS undefined/prototype behavior that Wasm cannot represent.
- Imported/scoped composition, broader nested ABI execution, malformed plans
  for remaining operations, official Deriving/TCOFloated and full gates.

No `Record.Unsafe` target implementation was added to the isolated package.
Reached foreign values still fail honestly. Do not replace them with recursion,
fabricated results or compiler-owned whole-library algorithms.

## Next binding direction investigated, not implemented

The small checked GC storage-provider boundary is in
`psrs-runtime/src/abi/storage/` and `psrs-runtime/src/storage/`, projected by
backend `bindings/runtime/` and `mir/lower/storage_call.rs`. It currently owns
array fill/read/write/trap and validates source types before raw ABI erasure.
Read the source contract before adding record roles; do not misuse its `Any`
trap-result category for arbitrary polymorphic record payloads.

An ordinary private constructorless `data Slot`, carried through existing
`Unsafe.Coerce`, successfully transported heterogeneous values in an Array:
Int 17, Unicode String, and a record whose recovered value was 42. Probe:
`/tmp/psrs-record-boundary/ErasedCarrier.purs`; actual runner evidence:
`/tmp/psrs-record-boundary/ErasedCarrier-run/run.json`. This suggests an explicit
erased payload carrier can use existing common representation semantics. It
does not establish a finished primitive contract or authorize arbitrary nominal
casts. Validate a semantic constructorless carrier and its actual erased shape,
not its module or textual name.

Possible small roles are record length, key-at, payload-at and checked building
from parallel arrays. Library searches/copy/update algorithms should stay in
`psrs-stdlib`. `PSRS.Ordering` and `PSRS.Array.Classes` offer private low-level
building blocks without importing Prelude/Ord back into `Record.Unsafe`; check
dependency cycles explicitly. Fresh-array write ownership, sorted unique keys,
nonnull fields and equal lengths need real validation. This remains design
exploration, not an accepted implementation plan.

## Roadmap state

Last read: Phase 3 #94 In Progress/0 corpus cases, #95 Blocked/1, #96
unstarted/0, #124 In Progress/0; Phase 4 #99 has 61 cases. #124's zero reflects
removing false standalone support-module blockers, not completion of all stdlib
APIs. #95's remaining `checkThrows` needs a real target exception-recovery
contract. Neither issue was closed and no comments were posted for this slice.
Refresh the board before selecting further roadmap work; local snapshots may
be stale. The broader project has meaningful work and is not globally blocked.

## Resumption progress, 2026-10-17 (later the same day)

First actions 1–4 are done, and action 5's first vertical slice is landed.
The control-pair repair was reviewed against the common planner and checked-plan
contracts and holds. With required Wasmtime the five cost tests, all 32 control
regressions and the record tests pass; backend tests, the full workspace,
formatting, source layout and Clippy pass (2,053 passed at the read/transport
checkpoint). The runtime board was remeasured: 276/340 at that checkpoint with
release suite `61d4dea7488511191630dcf2adb060f2de9d62701786893434b6b6f955212b7e`;
D-04, README and the runtime scoreboard record were promoted, and
[the open-record checklist](open-records.md) gained a post-repair checkpoint
section. Evidence lives under `/tmp/psrs-open-record-2026-10-17-*`.

The record-pattern slice (OR-05) followed: open-row patterns project by label
through one shared `project_labelled_field` kernel instead of guessing product
indices; the realize unit test, two required-Wasmtime source runs and the
official `FieldPuns`/`FieldConsPuns`/`ReservedWords` cases execute. The board
reached **279/340** on release suite
`7c9f8839dbd7cf73d63322e90ca2eefd5a904551c63cf73b76bb77b64673e2eb`, and the
debug workspace passed 2,056 tests. D-04 and README carried the 279/340 row.

Next, the whole dirty checkpoint was committed the same day under the topics
"Run the configured library runner…", "Check rank-N arguments…",
"Give open rows one immutable labelled record store", and
"Select open-row record patterns by label"; later inspections should state
their measurements against those commits rather than a dirty tree.

The open-row update slice (OR-06) then landed as a further commit: base and
update expressions lower exactly once in source order, the new owner
`Update` operation patches labels on one fresh store through the shared
helper machinery, missing runtime labels trap, and forged operands fail the
CC boundary. The board reached **282/340** on release suite
`8585a60b24a8a02eb8b5c132c596359422492e224f6199a133fefd2364476337`; the
official `ObjectUpdate`, `ObjectUpdate2`, `ObjectUpdater`,
`NestedRecordUpdate`, `NestedRecordUpdateWildcards` and derived-Functor
`2616` roots pass and the debug workspace passes 2,062 tests. D-04, README
and the runtime scoreboard record carry the 282/340 row.

This is still not topic acceptance: the remaining
slices are OR-08/OR-09
target bindings and the remaining gates.

## Follow-up agent start point, 2026-10-17 (evening)

Superseded by the OR-08/OR-09 resumption above: the slice named here as next
has since been implemented, validated and measured (284/340). The text below
is retained as the record of that start point.

Start from a clean checkout at `b7778658`; nothing is pushed and no issue was
closed or commented. Current accepted evidence:

- Runtime board **282/340** with all exits zero, 25 FFI groups excluded;
  failure decomposition, release suite binary SHA-256
  `8585a60b24a8a02eb8b5c132c596359422492e224f6199a133fefd2364476337`,
  Wasmtime 49.0.3, per-program rows in
  `/tmp/psrs-open-record-2026-10-17-runtime-281/summary.json` (earlier
  checkpoints 276 and 279 under
  `/tmp/psrs-open-record-2026-10-17-runtime{,-checkpoint-276}/`).
- Full debug workspace 2,062 passed / 0 failed / 5 ignored with
  `PSRS_REQUIRE_WASMTIME=1` and the isolated package
  (`/tmp/psrs-open-record-2026-10-17-update-workspace.log`).
- Frontend boards are unchanged by the update slice; their last full run is
  L1 904/908, L2 72/72 and passing resolution 403/413, L3 39/48, L4 39/50,
  L5 77/97 (`/tmp/psrs-open-record-2026-10-17-boards.log`).
- `RowPolyInstanceContext.purs` still fails P5 typechecking; its
  undetermined-variable pair prints in either order across repeated runs of
  the identical binary — presentation noise, not a changed reason.

Validation routine after any change (report any omitted scope honestly):

```sh
cargo fmt --all --check
PSRS_STDLIB_ROOT=/tmp/psrs-symbol-reification-stdlib PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Board re-measure procedure: rebuild the release suite binary
(`cargo test -p psrs-driver --test suite --release --no-run`), then measure
every program root with its own `PSRS_SUITE_FILTER` worker — three workers,
90-second cap, BigFunction under the separate 2 GiB RSS guard — reusing the
pattern of `/tmp/psrs-open-record-2026-10-17-measure.py` with a fresh output
directory. Land the new binary SHA-256, the changed failure set and the new
gate row in D-04, README and the runtime scoreboard record in the same commit
as the change, following [the authoring guide](../../authoring-guide.md).

Remaining open-record slices, ranked by unblocking value (requirements in the
[execution checklist](open-records.md)):

1. OR-08 source-typed runtime primitives and target bindings. This unblocks
   the three missing-foreign program blockers — `Record.Unsafe.unsafeGet`
   needs `Deriving.purs` and `TCOFloated.purs`, and
   `Test.Assert.checkThrows` needs `TCOMutRec.purs` — for three more board
   programs, and is the prerequisite for OR-09. Never fabricate a missing
   foreign's result: keep the source contract and implement the real target.
2. OR-09 library record algorithms with official `Record.Unsafe` wrappers
   under the vendoring contract, then OR-10 edge keys and unsafe-input
   contracts.
3. OR-07 broader generic/recursive/imported interactions and OR-12 broader
   canonical-ABI coverage as integration checks over the new update and
   pattern paths.
4. OR-11 remaining malformed-IR rejection surfaces, then OR-13's full boards
   to close the topic.

Also still unsupported at the surface: pattern-driven tail binding
(`..rest`), recorded in OR-05's row.

Guardrails that bound every slice: preserve official standard-library sources
and the store's honesty contracts; stage and runtime correctness are separate
claims; a local test passing is not acceptance — re-run the affected
scoreboard and record the number before closing anything; do not narrow the
design in [open records](../../design/backend/fp/open-records.md) to match an
implementation. `#124` stays In Progress on the board while these slices run.

## OR-08/OR-09 resumption evidence, 2026-10-17

The current compiler work resumes from `b7778658` in the existing checkout;
compiler source and test changes are in progress there. The standard-library
package is now pinned at `e556bf2f8cb377c049d2ff0a5a97b484c7b36bc5`, with
source fingerprint `fnv1a64-v1:7d96f1c44d8d52cc`. The package checkout
`/tmp/psrs-open-record-stdlib-or09` is clean at that revision. The lock retains
the repository's sibling path `../psrs-stdlib`; use
`PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09` for local compiler
commands so they consume the pinned package without changing the shared sibling.

OR-09 has focused source and runtime evidence plus the fresh suite-board
measurement:

- The final vendoring audit covers 50 packages and 309 modules, with no
  upstream modules absent from the vendor. Its inventory and source comparison
  are in `/tmp/psrs-open-record-or09-evidence/final-audit/`.
- The Node conformance suite passed 13/13 cases. The independent target oracle
  report `/tmp/psrs-open-record-or09-evidence/oracle-edge-final/record-unsafe.json`
  accepts the shared values case and the empty, missing, and inherited-key
  trap cases. It records the target differences explicitly: Wasm has no
  JavaScript `undefined` value or object prototype, and `__proto__` is an
  ordinary own key in target records. The report uses CLI SHA-256
  `d91a5b9943b871e4a90997ce0f0fde2123336aa248a7ca12272cd690425dceac` and
  confirms the package did not change during the run.
- With the same CLI and package, the official `Deriving.purs` and
  `TCOFloated.purs` program roots each compiled and ran through the configured
  library runner with exit 0 and exact stdout `Done\n`. Separate compile and
  execution records are in `/tmp/psrs-open-record-or09-official-Deriving/`
  and `/tmp/psrs-open-record-or09-official-TCOFloated/`. This resolves the two
  `Record.Unsafe.unsafeGet` root blockers in focused evidence; it does not
  establish the full L6/M7 board. The emitted Wasm SHA-256 values are
  `4227351f326c51abcbe021a3911b3b173676d4e71c2b5f68535df2c24fb4e98c`
  (`Deriving`) and
  `b4eefee336e38789268930b32854a26acc3e0f4cb6f7224cd34b151ce5c9b9c1`
  (`TCOFloated`). `TCOMutRec.purs` still needs a real
  `Test.Assert.checkThrows` target contract.
- The three-module independent caller report
  `/tmp/psrs-open-record-or08-independent-run-fixed/run.json` executes with
  exit 42 using the earlier package fingerprint
  `fnv1a64-v1:5fcbb57525c63eb1`. It is direct read/transport evidence, separate
  from OR-09 and from the official-suite board.

The fresh release-board run over all 365 program roots measured **284/340**
with all exit-zero completions and 25 FFI groups excluded
(`/tmp/psrs-open-record-or08-or09-runtime/summary.json`). Exactly
`Deriving.purs` and `TCOFloated.purs` leave the failing set — the two
`Record.Unsafe.unsafeGet` blockers above — and no other failure changed
state; the remaining 56 roots are 31 P5 typecheck, 11 P5 kind check, 5 P8
closure/linking (3 closure capture, 1 recursive local binding,
`Test.Assert.checkThrows`-blocked `TCOMutRec.purs`), 5 P3 resolve, 2 P0 lex
(DEC-16), one 10-second runtime timeout, and the bounded BigFunction
compilation, which reached the measurement cap near the 2 GiB guard before a
diagnostic. OR-09 does not close the topic: OR-07 broader
generic/recursive/imported interactions, OR-10 edge-key and unsafe-input
coverage, OR-11 malformed-IR surfaces, OR-12 broader canonical-ABI coverage,
and OR-13 full-board acceptance remain open. `#124` remains In Progress; no
push, issue comment, or closure was made for this resumption.

### Validation checkpoint: integration fixes and final rerun

With `PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09` and
`PSRS_REQUIRE_WASMTIME=1`, formatting passed. The first workspace run had 523
passes and five failures because legacy ABI, coverage and uninhabited-layout
fixtures lacked the newly required `ordinary_data_ids` evidence; the fixtures
were updated to provide the valid producer metadata. The next run reached the
source-layout tests after all 529 backend tests passed, then rejected new flat
siblings for the record-build and operations modules. Those files were moved
into their owning module directories; the focused source-layout test passes
1/1 and direct record-build execution passes 2/2. These were fixture and source
organization repairs; the verifier requirements remain intact.

An integrated workspace run passed 2,077 tests, with zero failures and five
ignored, using the new package; its log is
`/tmp/psrs-open-record-or08-or09-workspace-integrated.log`.

The final-tree validation is accepted. Formatting and source layout pass, and
Clippy passes with warnings denied. The full debug workspace with
`PSRS_REQUIRE_WASMTIME=1` and the new package passes 2,084 tests, failing
zero, ignoring five
(`/tmp/psrs-or08-final-tree-workspace.log`). The release suite rebuilds with
SHA-256
`37bcdd007466f2ed91f2cbd06962c584bcdd00caef10f73465a5070f36d1f4ee`, and the
board re-measurement landed as the 284/340 row above and in
[D-04](../../design/D-04-suite-roadmap.md),
[README](../../../README.md), and
[runtime-program-scoreboard](runtime-program-scoreboard.md). Frontend boards
were not rerun; the slice changed only backend, runtime and library content,
and their numbers are unchanged from the 2026-10-17 frontend confirmation.

### Scope of the opening checkout state

The opening `State` and `Committed state` paragraphs describe only the earlier
`b7778658` checkpoint. The compiler follow-up and lock update above it are now
committed, and the resumption sections describe that state. The separate
library package at `e556bf2f` is a clean local commit in
`/tmp/psrs-open-record-stdlib-or09`; it has not been pushed. Take the accepted
final-tree validation and board numbers from the resumption sections, not
from the opening snapshot's 282/340 row, which belongs to the earlier package
and source state.

## Partial-exhaustiveness slice resumption, 2026-10-17 (S1 landed, S2 blocked): in-progress checkout

This section supersedes the "Next move" lists above for the current checkout:
`master` is at `7dd45e4b` and the compiler work for the checker-side
`Partial` flow is present as **uncommitted changes** (29 modified files plus
untracked `crates/psrs-coverage/`,
`crates/psrs-typecheck/src/typecheck/infer/coverage.rs`, and a temporary
repro test). Nothing is pushed; `#124` remains In Progress; the existing
stashes on the `stdlib/vendor-core-libraries` history predate this work and
must be left alone.

### S1 is complete and verified in the working tree

- `crates/psrs-coverage/` is the single owner of usefulness analysis:
  `src/{lib,pattern,types,engine,analyze,render,tests}.rs` port the backend
  engine verbatim, including the two top-level guards
  (`all-Any` row short-circuit and the `(matrix, query)` active-state scoped
  set) that make the uninhabited recursive-product path terminate — omitting
  either stack-overflowed that test. Registered in the workspace `Cargo.toml`
  members and `[workspace.dependencies]`.
- The backend adapter `crates/psrs-backend/src/cc/case/coverage/mod.rs` is a
  thin shim keeping `analyze(&Module, TypeId, &[CaseBranch]) -> CoverageReport`;
  the old backend `engine.rs`/`render.rs` were deleted and
  `decision`'s `canonical_literal` re-export trimmed. Before the S2
  instrumentation described below, `cargo test -p psrs-backend` passed 535/0
  and clippy (`-p psrs-coverage -p psrs-backend -D warnings`) was clean.

### S2 checker integration is functionally in place (with instrumentation still in the tree)

- `crates/psrs-typecheck/src/typecheck/infer/coverage.rs` (new, registered in
  `infer/mod.rs`; `psrs-coverage.workspace = true` added to the crate's
  `Cargo.toml`) interns `InferType` into skeleton ids
  (`Constructor/Applied/ForAll/Constrained/RowExtend/RowEmpty/`...`/Unknown`)
  and implements `psrs_coverage::Types`. Constructor rows are elaborated
  lazily to a fixpoint with `instantiate_constructor` + `resolve_type` on
  throwaway fresh unknowns; coverage never unifies anything.
  Two landmines already found and fixed:
  - Types with **zero** constructors (uninhabited/foreign) must still be
    recorded as examined (`rows.insert(type_id, Vec::new())`) or the fixpoint
    scan livelocks;
  - `record_fields` answers the **known field list for open-row tails** (a
    row-variable tail means the record may grow beyond the pattern's labels,
    which the listed pattern still covers). Closed-record handling in the
    *backend* adapter must not change; there the rows are already solved.
    Without this, every `{ val, acc }`-style partial record pattern in the
    standard library (where-bound `go` records) reports a bogus missing
    case.
- `infer/case.rs` replaces the given-only fallback block: a non-empty case
  whose analysis is non-exhaustive pushes one report-only `Partial` wanted
  carrying `witness: Option<Vec<Vec<String>>>` (new field on
  `WantedConstraint`, set `None` at every other construction site) and always
  attaches the guard-desugar-style `PartialFallback` branch; the old
  given-in-scope attach remains as the conservative path. Empty cases (no
  branches) still take the backend trap path, deliberately out of this
  slice's scope.
- `classes/evidence/finalize.rs::abstract_dictionaries` now **skips
  report-only constraints** (via `prim::is_report_only`) — no dictionary
  parameter is abstracted and no `Abstracted` solution is recorded for them.
  Retaining a `Partial` constraint previously abstracted an empty-record
  dictionary parameter, which shifted every value's application shape in
  THIR and broke the open-record regression test; that skip is what makes
  retained `Partial` schemes honest AND layout-neutral.
- Solver facts confirmed by experiment (do not re-litigate): report-only
  classes do not precede givens, so `unsafePartial` residents discharge by
  given lookup first; the retention hook at
  `classes/solve/search.rs` (`is_report_only && policy_keeps_unsolved`)
  already retains arity-0 `Partial` under a `Retain` policy, so
  `solve_partial` is deliberately untouched; `RequireSolved` (annotated
  declarations) reaches the rule and reports. The single wording point is
  `prim/reports.rs::unresolved_report_error` (extend it with the official
  "A case expression could not be determined to cover all inputs. ..." text
  from the wanted's `witness`), with the `NoInstance` error at
  `classes/solve/entry.rs` as the backstop for the same message.

Observed end-to-end with `PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09`:
`f x [y, z] = x y z` (argument-pattern lambda) now compiles through P8 —
retained `Partial`, no dictionary parameter, fallback reaches the backend
matrix. An annotated non-exhaustive case rejects at P5, currently with the
generic `no instance for constraint Partial` wording (official wording is
the pending reports.rs change). The `unsafePartial` probe showed five
standard-library `Partial` diagnostics **before** the open-row `record_fields`
fix above; it has not been re-verified after that fix because the blocker
below interrupts compilation. `cargo test -p psrs-typecheck` passes 135/0
including `infers_an_open_record_row_from_a_partial_record_pattern`.

### Blocking disorder: stack overflow in the backend CC layout pass

Compiling prelude + `/tmp/psrs-open-record-stdlib-or09/lib/Data/List/Types.purs`
overflows the stack (default 8 MB; a 512 MB thread completes, so the
recursion is pathological/deep rather than infinite). Reproduction test
(new coverage instrumentation to remove at cleanup):
`crates/psrs-driver/tests/repro_coverage_overflow.rs` with `PSRS_COVERAGE_ON=1
cargo test -p psrs-driver --test repro_coverage_overflow`.

Bisection evidence, all via temporary eprintln probes listed below:

1. Typecheck side: all 472 checker `case_coverage` analyses enter and
   complete; the interner stays tiny; the engine's `useful_inner` gets
   called once per analysis.
2. Frontend pipeline (`lowering.rs`): resolve, typecheck, per-module Core
   lowering, link, and Core verification all complete
   (`core-verify-done frontend-ok`).
3. Backend pipeline (`pipeline/mod.rs`): core optimization and external
   binding extraction/conformance complete; the crash occurs inside
   `cc::lower_module_with_relations` BEFORE any case is compiled
   (`DBG-CC-CASE` never prints), after primitives, local-row
   instantiation, and type ids are registered and while iterating layout
   work: the last printed marker is
   `DBG-LAY append-function-types-begin`, so the overflow is inside or
   directly around `cc/layout/functions.rs::append_function_types`
   (between it and `aggregate::normalize_aggregate_layouts`).
4. The abort shows a hashbrown layout calculation with astronomic values
   (~4.3e9 buckets), and the backtrace collapses to one frame — instrument
   instead of trusting post-abort traces.
5. Negative control that explains earlier confusion: the env-gated
   "pretend every case is non-exhaustive" mode also reached many P5
   reports and dropped out before the backend ever ran, so its earlier
   passing runs say nothing about the backend path.

Working hypothesis to test first: the checker now attaches the
`PartialFallback` branch (whose value is a nested `case <int> 0 of` with
zero branches) to genuinely non-exhaustive cases across the whole standard
library; those newly embedded nested cases change the core declarations the
function-signature pass walks. Confirm the actual recursion
(suspects: function-type typing recursion or an `HashSet`/`HashMap` keyed
insertion loop in that pass or its `functions.rs` helpers, e.g. signature
interning over deeply nested arrow/case types) before changing code; a
targetContentLoaded comparison of core tables across this module with
`PSRS_COVERAGE_ON` off — where P5 diagnostics abort before the backend —
will isolate whether the *core* input differs at all from pre-slice state
for the cases that survive typecheck.

### Instrumentation to remove before any commit

All temporary debug markers below must be removed before any commit:

- `crates/psrs-typecheck/src/typecheck/infer/case.rs`: the
  `PSRS_COVERAGE_ON` / `PSRS_ANALYZE_NOOP` / `PSRS_WITNESS_ONLY` /
  `PSRS_FALLBACK_ONLY` env-gated analysis selection; restore the plain
  `self.case_coverage(...)` call.
- `crates/psrs-typecheck/src/typecheck/infer/coverage.rs`: `DBG-COV`,
  `DBG-COV-CALL`, `DBG-INTERN` eprintlns, the `debug_skeleton` helper, and
  the `static CALLS` counter.
- `crates/psrs-coverage/src/engine.rs`: the `static CALLS` counter and
  `DBG-ENGINE` eprintln in `useful_inner`.
- `crates/psrs-typecheck/src/typecheck/classes/solve/entry.rs`:
  `DBG-ENTRY` markers (both the retained and no-instance branches).
- `crates/psrs-typecheck/src/typecheck/prim/dispatch.rs`: the
  `DBG-DISPATCH` marker.
- `crates/psrs-driver/src/program/lowering.rs`: all `DBG-PHASE` markers.
- `crates/psrs-driver/src/program/library.rs`: the `DBG-LIB-ORIGIN`
  marker inside `shift`.
- `crates/psrs-driver/src/prelude/mod.rs`: the stray
  `#[cfg(feature = "never")] debug_print_library(...)` line (emits an
  unexpected-cfg warning).
- `crates/psrs-cli/src/build.rs`: the `DBG` marker above the
  library-diagnostic print.
- `crates/psrs-backend/src/cc/mod.rs` (`DBG-CC*`),
  `crates/psrs-backend/src/cc/layout/mod.rs` (`DBG-LAY*`),
  `crates/psrs-backend/src/cc/case/mod.rs` (`DBG-CC-CASE`),
  `crates/psrs-backend/src/pipeline/mod.rs` (`DBG-PHASE`).
- Delete `crates/psrs-driver/tests/repro_coverage_overflow.rs`.

After cleanup re-run the real-flow probes (`f x [y, z] = x y z`
acceptance, annotated rejection wording, `unsafePartial` acceptance), the
three targeted `Let.purs`/`ShadowedTCOLet.purs`/`Where.purs` source tests,
then the slice's standing plan: extend `unresolved_report_error` and the
`entry.rs` fallback with the official wording, full workspace validation
(`cargo fmt --all --check`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`PSRS_REQUIRE_WASMTIME=1`), release suite rebuild + SHA-256, and the full
board re-measure (worker pattern
`/tmp/psrs-open-record-or08-or09-measure.py`, 3 workers, 90 s cap + RSS
guard) expecting 287/340. Keep the same-commit discipline: design-document
change, code, and tests land together per topic.

## Partial-exhaustiveness follow-through

The current checkout preserves the original S1/S2 changes and completes the
checker integration without environment-gated semantics. All listed temporary
instrumentation and the temporary overflow test were removed. No branch,
worktree, push, or issue closure was made. The isolated library remains clean
at `e556bf2f` with the existing lock fingerprint; no official source was edited.

### Repairs to the blocked checkpoint

1. The overflow was an actual transparent newtype cycle in official
   `Data.Void` (`newtype Void = Void Void`), rather than the coverage engine or
   a finite deep arrow. Signature tracing showed `scalar_type` repeatedly
   expanding the same declaration. The common uninhabited-storage predicate
   now recognizes transparent declaration cycles, including mutual cycles,
   and uses erased references. Conversion stops at that boundary, and integer
   box reservation includes such erased slots. Direct layout tests cover self
   and mutual cycles; a source execution coerces a boxed scalar through a
   cyclic newtype constructor pattern and recovers 42. Constructor rows remain
   available to coverage: representation uninhabitedness must not erase the
   source pattern signature.
2. The S2 skip in `abstract_dictionaries` was inconsistent with signature and
   use-site elaboration. It was removed: inferred and declared `Partial`
   constraints use the existing common dictionary convention. Skipping only
   abstraction caused `invalid THIR: global reference is not a valid scheme
   instance` when an inferred partial function was actually called. The new
   array-argument-pattern test calls it under `unsafePartial` and exits 42;
   the existing explicit-Partial test still checks both 42 and a trap.
3. Every nested pattern type is resolved after all branches have unified,
   before the checker adapter interns it. Resolving only the scrutinee left
   stale unknowns in the engine's exemplar-field types. Official
   `Data.List.Lazy.drop` then incorrectly demanded `Partial` for complete
   `Nil`/`Cons` alternatives. A reduced recursive two-argument stepper executes
   with result 42, and the original guarded-library typecheck passes.
4. Missing-case witnesses now reach the shared `NoInstanceFound` report
   formatter and the solver's fallback. Annotated cases reject at P5 with the
   official explanatory wording and exact source span. Inferred declarations
   retain `Partial`, and authorized fallbacks keep first-match behavior through
   Core/CC. The report prints the first witness plus an ellipsis, not a claim
   to enumerate all missing cases. Synthetic multi-scrutinee products still
   render as a record witness; exact official multi-column pretty printing is
   not established by the error-code scoreboard. Empty source cases remain
   outside this slice. The local stepper fixture now uses primitive `Partial`; a user class
   merely named `Partial` cannot authorize non-exhaustiveness.

The original complete-`Data.List.Types` probe now terminates on its small
thread stack and reports the separate P8 limitation `recursive local binding
is not a function`; this is not compile acceptance for that whole module.
Evidence: `/tmp/psrs-coverage-resume-fixed.log`.

### Acceptance evidence

The final release suite SHA-256 is
`bab10d4c7480c7db64fce9dacba8c96c035f9e760a810679a9d2a750a59233a3`.
The full frontend remeasurement is L1 904/908, L2 72/72 with passing resolution
403/413, L3 39/48, L4 39/50, and L5 **81/97** (`NoInstanceFound` **52/53**).
Relative to the same-package baseline, `2806`, `4466`,
`NonExhaustivePatGuard`, and `Superclasses5` now agree; no L1–L4 number moves.
Evidence: `/tmp/psrs-partial-frontboards-final.log` and the baseline
`/tmp/psrs-partial-frontboards-baseline.log`.

The final runtime board is **287/340**, all successes exiting zero, with 25
FFI groups excluded. Exactly `Let`, `ShadowedTCOLet`, and `Where` recover;
there are no regressions. The remaining 53 split into 31 P5 typecheck, 11 P5
kind check, 2 P8 closure/linking, 5 P3 resolve, 2 P0 lex, one runtime timeout,
and bounded BigFunction compilation. The guarded BigFunction run times out at
90.89 seconds with peak RSS 1,485,792 KiB; the 2 GiB guard stays enabled.
The worker manifest contains the 364 ordinary completed rows plus that
separate guarded result after the sandbox blocked the first RSS query.
Evidence: `/tmp/psrs-partial-final-runtime/summary.json` and
`big-function-checked.json`. Formatting, source layout, and strict workspace
Clippy pass. The final full workspace with
`PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09` and
`PSRS_REQUIRE_WASMTIME=1` passes **2,093 tests, zero failures, five ignored**;
see `/tmp/psrs-partial-workspace-final.log`. Relative to the previous 2,084-test
log, the inventory adds one backend cycle-layout test, three shared-engine
tests, and five driver regressions. No prior tests were removed from that
inventory. All five ignored scoreboard groups were exercised separately by
the release frontend boards and per-program runtime run.

Formatting and strict Clippy use the standard full-workspace commands. The
original seven driver failures in the first integration attempt were four
outdated backend-diagnostic assertions, one fixture that shadowed primitive
`Partial`, one confirmed nested-pattern-type failure in the original library case, and
one handler integration failure. Their corrected paths all pass in the
final full run. The superseded first workspace/board runs were stopped; only
the final evidence paths above establish acceptance.
The open-record topic remains open: retain the OR-07, OR-10, OR-11, OR-12 and
OR-13 obligations in the checklist rather than treating this frontend slice
as their complete acceptance.

## Generic local rows and nested ABI follow-through

The current continuation preserves the earlier coverage/Partial changes and
removes an obsolete correctness dependency in P8. CC no longer calls
`instantiate_local_rows`; the dedicated finite local-row pass and its private
expression-instantiation helper are removed. Normal declaration specialization
and its capture-avoiding substitution remain available to Core optimization.
The representation owner, generic call adapters and ordinary Core verification
now carry this boundary directly, as required by DEC-22 and the open-record
design. No library source, lock, branch, commit, push or issue state changed.

### Focused evidence and test ownership

- OR-07: `tests::records::open_rows` passes 17/17 with required Wasmtime. The
  new `composition` module adds four tests: unoptimized Core with one captured
  generic reader shared by two distinct closed rows; imported recursive
  type-changing updates with retained nested callable/Array/ADT payloads;
  a captured local polymorphic reader passed at an open higher-order use type;
  and captured recursive local updates used at two different tail shapes.
  Each program exits 42. The first test explicitly bypasses Core optimization
  and counts the generic reader before executing the emitted artifact.
- Baseline: `/tmp/psrs-or07-before-local.log` and
  `/tmp/psrs-or07-without-local-preparation.log` both execute the existing local
  captured-row reproducer. Negative control:
  `/tmp/psrs-or07-specialization-negative-control.log` fails the new structural
  assertion with the old preparation restored; current focused acceptance is
  `/tmp/psrs-or07-all-source.log`.
- OR-11: the public CC verifier rejects wrong operand types, missing operand
  declarations, wrong result types and inconsistent embedded destinations for
  all five read/update operations. Its expected types are specified separately
  from the implementation helpers; diagnostics retain the original span.
  Evidence: `/tmp/psrs-or11-operand-matrix.log`.
- OR-10: all five read/update operations execute both valid and mismatched
  label/payload-array inputs. Each malformed input traps before any later
  unpack, including an ignored result and an empty Update. Physical CC/MIR
  verification succeeds: these are runtime-content tests, not malformed
  representation-schema tests. Evidence:
  `/tmp/psrs-or10-malformed-store.log` (ten executions).
- OR-12: the new filesystem test sets exact access/modification timestamps,
  reads nested `Either`/`Maybe` record payloads, transports the descriptor
  record through `Box (Array a)`, updates its size through an open row while
  retaining timestamps, then sends the recovered timestamps through the ABI
  again. Source assertions verify all scalar fields and the unchanged input;
  host metadata independently verifies the outbound modification timestamp.
  Evidence: `/tmp/psrs-or12-timestamp-roundtrip.log`.

### Current validation

Formatting and strict workspace Clippy pass at this continuation:
`/tmp/psrs-or07-fmt.log` and `/tmp/psrs-or07-clippy.log`. The new release suite
is `/tmp/psrs-or07-suite`, SHA-256
`52a9b77e706fd2030ed8c235c75f3a313ee118a18de0b49e4be21eb375e03df1`.
Full workspace validation passes **2,100 tests, zero failures and five
ignored** (`/tmp/psrs-or07-workspace.log`), separately from the historical
2,093-test checkpoint. All five ignored scoreboard groups execute separately
in the frontend/runtime remeasurements recorded below. The seven additions
are four open-row composition tests, one nested ABI test, one public CC
negative matrix and one direct GC runtime-content matrix. No existing test
was removed. The driver unit group passes 814/814, and all six locked
control-reference integration tests execute and pass. The same clean package `e556bf2f` /
`fnv1a64-v1:7d96f1c44d8d52cc` is selected via
`PSRS_STDLIB_ROOT=/tmp/psrs-open-record-stdlib-or09`.

The current CLI also reruns the library-owned `Record.Unsafe` oracle:
`/tmp/psrs-or07-record-oracle/record-unsafe.json` accepts its value program and
all three explicit empty/missing/inherited get traps; the Node conformance
suite passes 13/13 (`/tmp/psrs-or07-node-conformance.log`). This refreshes the
previous OR-09/OR-10 oracle against the current compiler instead of relying
only on its old artifact. The package fingerprint stays unchanged.
The rebuilt frontend scoreboards also finish with unchanged L1 904/908,
L2 72/72 (passing resolution 403/413), L3 39/48, L4 39/50 and L5 81/97
(`NoInstanceFound` 52/53): `/tmp/psrs-or07-frontboards.log`.

The current runtime board is **287/340** with no success-set changes. The
manifest combines 364 completed ordinary roots (25 FFI exclusions) with the
separate bounded BigFunction result. BigFunction reaches the 2 GiB RSS guard
after 27.89 seconds, with observed peak 2,141,824 KiB; this supersedes the
previous checkpoint's 90.89-second timeout for the current artifact, without
claiming BigFunction compile acceptance or a performance repair. The remaining
53 still split as 31 P5 typecheck, 11 P5 kind check, 2 P8, 5 P3 resolve,
2 P0 lex, one runtime timeout and BigFunction. Evidence:
`/tmp/psrs-or07-runtime/{summary.json,results.jsonl,big-function-checked.json}`.

### Next topic acceptance review

Start from the committed checkpoint above and preserve the clean development
package. The concrete OR-07 prerequisite and the previously listed operation/ABI test gaps are now
covered. Before declaring the whole topic complete, review OR-01–OR-12 as one
integrated contract, including the remaining OR-04/OR-05 interaction language,
and update each requirement's status from its actual source/direct-IR/runtime
evidence. Do not infer full topic acceptance from the 287/340 corpus count.
Do not change official source or substitute `Test.Assert.checkThrows` merely to
improve that count; its target behavior remains independently unimplemented.

The final slice review confirms that the removed expression-substitution path
was used only by the deleted local-row pass; the declaration-specialization
path retains its previous capture-avoidance and quantified-binding behavior.
All changed/new Rust files meet the 500-line rule. `git diff --check` passes.
The source, tests and corresponding design/checklist updates are now committed
as `e5972241` and `e331f9ba`; no existing changes were discarded. The opening
handoff snapshot records their exact state and the outstanding topic review.
