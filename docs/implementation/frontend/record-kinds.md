# Record Kind Contract Acceptance

**References:** [kinds](../../design/frontend/type-system/kinds.md),
[rows and records](../../design/frontend/type-system/rows-and-records.md),
[D-04](../../design/D-04-suite-roadmap.md).

**Status:** Verified for the record-field kind contract in #167.

The review starts from `ea608df0`, preserving the pre-existing untracked
`output/` directory and locked standard library. The official `4105` program
rejects in P5 with `QuantificationCheckFailureInType`, as it also does at the
earlier unmodified `509c3067` checkpoint. Its `Lib` support source defines
`Template col = { bio :: col String }`, `Identity a = a`, and
`Patch = Template Identity`.

## Contract and correction

The record shorthand path inferred a shared field kind but never constrained
it to `Type`. That gave `Template` an erroneous polymorphic result kind for
`col`. A later `Template Identity` use therefore left a kind unknown inside
the closed synonym body. Quantification correctly rejected that unknown;
suppressing its diagnostic would conceal the earlier defect.

Rows and records now share row-field and tail inference. Record shorthand
checks that inferred row against the authoritative primitive kind of `Record`,
just as an explicit application does. No kind quantification rule, library
source, or per-name exception changes.

## Evidence

| Requirement | Evidence | State |
| --- | --- | --- |
| Ordinary record fields have kind `Type`, including through higher-kinded aliases | Native `record_fields_constrain_higher_kinded_aliases_to_type`, official `4105` input closure | Verified |
| Shorthand, explicit `Record` application and incompatible tails reject non-Type fields | Native `a_record_field_cannot_have_symbol_kind`; three purs differential cases in `tests/upstream/record_kinds.rs` | Verified |
| Bare rows retain arbitrary field kinds | `Row Symbol` positive control in `tests/support/record_kinds.rs`, independently accepted by purs | Verified |
| Imported synonym/record consumers execute | `imported_record_synonyms_and_the_official_consumer_execute`: value-sensitive consumer exits 42; unchanged official `4105` exits 0 and prints `Done`; mandatory Wasmtime | Verified |
| Quantification rejection and whole-workspace integration remain correct | All 67 native kind tests and 23 purs differential test functions pass; full workspace (2336 passed, zero failed), frontend boards and complete runtime remeasurement pass | Verified |

The before/after diagnosis uses the same root program, explicit `--input` for
`4105/Lib.purs`, locked package and 30-second compiler timeout. The snapshots
are `/tmp/psrs-kind-instantiation-program-{before,after}.json`; the compatible
comparison `/tmp/psrs-record-kind-compare.json` records recovery from P5 rejection
to completed compilation. No canonical artifact-content or expression-lineage
comparison is claimed by the compile diagnosis.

Final formatting, default Clippy and `cargo test --workspace` pass: 2336
passed, zero failed, six ignored boards. The five frontend boards execute
separately with unchanged results: L1 904/908 (the four DEC-16 differences),
L2 76/76 and 403/413 passing resolution, L3 50/50, L4 65/65, L5 101/101 and
roles 12/12.

The frozen release harness `/tmp/psrs-record-kind-suite-final-v2`, SHA-256
`7cb60b884dcd54386c70446a7bc161a70fe1277b28dc45e0d78707211ab93c5b`,
executes all 365 runtime roots serially on 2026-10-23 (Asia/Shanghai):
**300/340**, 25 FFI exclusions, all agreeing exits zero. Only `4105` recovers
from the previous 299/340 board; no program loses agreement. The remaining
40 roots are 26 P5 typecheck, 3 P5 kind check, 5 P3 resolve, 1 P8 closure
conversion, 2 P0 lex, the `TCOMutRec` assertion trap, the `2288` guest timeout
and BigFunction's incomplete compiler allocation verdict. The library lock,
Wasmtime 48.0.1, 10-second guest timeout, 524288-byte engine stack and
BigFunction compiler-only 8 GiB bound are unchanged.

The exact settings, per-program evidence and name-set comparison are in
`/tmp/psrs-record-kind-runtime-board-final/{measurement,results,summary}.json`.
Final workspace/frontend logs use the `-final.log` suffix; the earlier
interrupted runs are separate observations. Broader kind, row and backend
feature coverage remains separate from this record-field contract.
