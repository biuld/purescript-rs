# WIT Aggregates in Erased Payload Slots

Measured on 2026-10-07.

**Design:** [Runtime representation and checked boundaries](../../design/backend/fp/representation-and-evidence.md#aggregates-in-bare-polymorphic-slots)

## Starting point and boundary

Baseline: `11db1757c20b76b5231e033395f000714f279baa`, branch
`stdlib/vendor-core-libraries`, clean worktree. The locked standard library
was unchanged throughout this compiler repair.

`PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi:: -- --nocapture`
ran 145 tests: 142 passed and three trapped with `cast failure`:

- `filesystem::stats_and_reads_a_directory_through_preopens_when_wasmtime_is_available`
- `filesystem::writes_and_reads_a_file_through_preopens_when_wasmtime_is_available`
- `wrappers::reads_stdin_with_blocking_read_when_wasmtime_is_available`

The minimal source reads five stdin bytes through `WASI.IO.blockingRead`,
extracts `Right bytes`, and returns `arrayIndex bytes 0`. Both before and after
compile diagnosis accepted this source. Identical source inputs and the locked
library were confirmed by `diagnose --compare`; no observed pass-status change
was reported. Compile acceptance did not establish runtime correctness.

The mismatch was at canonical ABI decoding into the generic variant field.
The decoder constructed a specialized array or record and cast its reference
into the erased slot. Source constructors normalize these values into the
aggregate owner's canonical erased-element or erased-field storage protocol.
The checked consumer therefore tried to recover a protocol value from a
specialized aggregate.

## Repair

- CC and canonical ABI adapters share `cc::payload::PayloadPlanner` for recursive
  scalar boxing and array/record normalization and recovery. Nominal references
  retain their identity behavior.
- Decoded aggregate payloads normalize before entering bare erased fields.
  Variant parameter lowering recovers the concrete checked value before ABI
  flattening. Conversion control-flow exits are propagated to their consumers.
- ABI projection reachability traverses the same storage conversion plans before
  target layouts are assigned, retaining protocol layouts and required boxes.
  Missing owner protocols produce an error.
- Variant parameter branches pass call-local buffer pointers, byte lengths and
  element counts through the merge block. Inactive cases pass null and zero;
  post-call cleanup no longer uses branch-local values outside their dominance
  scope. Nested variant cleanup composes through the same merge mechanism.

## Evidence and limits

Runtime evidence requires Wasmtime, using version `49.0.2` in this run. The
existing stdin test asserts `hello\n`; file tests assert file contents and
console output, including nested record/option payloads. The new
`payloads::wit_result_bytes_recover_values_and_empty_arrays_when_wasmtime_is_available`
asserts empty-array length and exact byte values `0`, `255`, `128`, `42`, then
returns 42 with empty stdout and stderr. Arbitrary bytes are tested as
`Array Int`, without requiring valid UTF-8.

Three backend regressions cover record and record-array ABI result-to-parameter
round trips and rejection of a missing owner protocol. These lower, optimize,
encode and validate Wasm; they do not execute a synthetic host import. Actual
runtime evidence comes from the driver tests above.

Focused validation commands:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi:: -- --nocapture
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend --lib cc:: -- --nocapture
cargo test -p psrs-backend --lib mir:: -- --nocapture
cargo test -p psrs-cli --test source_layout
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

The final WASI selection passed 146 tests (the original 145 plus the new byte
payload regression), with zero failures or ignored tests. The standalone stdin
reproducer returned 104 for `hello`, with empty stdout and stderr. Its Wasm
SHA-256 was `a9a672b121379ebd7c3cd26518953bb9c5365fd3ff6a0382b75e0e4003fa7dbf`.

The CC selection passed 114 tests. The MIR selection passed 188 tests, including
all three new regressions. Source-layout validation, formatting, and workspace clippy passed.
Full workspace tests and corpus scoreboards were
not run; the user requested focused validation. This is acceptance for this ABI
repair, not completion of the broader backend or unrelated Show, row-evidence,
or ambiguity work.
