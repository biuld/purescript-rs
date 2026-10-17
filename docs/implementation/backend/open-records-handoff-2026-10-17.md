# Open Records and Roadmap Handoff — 2026-10-17

**State:** Paused at the user's request. Continue in the current dirty checkout.
The project goal and the open-record topic are incomplete. No compiler commits,
pushes or pull requests were made during this work. Do not treat this document
as integrated acceptance.

## Checkout and package ownership

- Compiler: `/Users/biu/Projects/purescript-rs`, branch `master`, HEAD
  `fe6dd78f14d5f4913e78d2d30c57e3b38cc5c900`.
- All compiler work described below was uncommitted at the pause, including
  new files and
  module-directory moves. Preserve the entire dirty tree.
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
is now **279/340** on release suite
`7c9f8839dbd7cf73d63322e90ca2eefd5a904551c63cf73b76bb77b64673e2eb`, and the
debug workspace passes 2,056 tests. D-04 and README carry the 279/340 row.

This is still not topic acceptance: the next
slices are OR-06 open-row immutable updates (preserving unknown tails, exact
evaluation order, including `ObjectUpdate.purs` and
`NestedRecordUpdateWildcards.purs` which still reject honestly), then OR-08/OR-09
target bindings and the remaining gates. All work described above, including
this document, was committed the same day under the topics
"Run the configured library runner…", "Check rank-N arguments…",
"Give open rows one immutable labelled record store", and
"Select open-row record patterns by label"; later inspections should state
their measurements against those commits rather than a dirty tree.
