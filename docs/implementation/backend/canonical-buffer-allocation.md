# Canonical Buffer Allocation and Lifetime Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Canonical buffer allocation and lifetime](../../design/backend/wasm/canonical-buffer-allocation-and-lifetime.md),
with [DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md).

**Progress:** Split from
[linear memory and canonical ABI](linear-memory-and-canonical-abi.md) so that
buffer allocation and lifetime are tracked on their own. `cabi_realloc` is now a
reclaiming aligned allocator with free-list reuse and coalescing; call-local and
import-result buffers are freed at the boundary; the heap-state region and
allocator provenance are modeled. `post-return` remains Blocked because no
current export returns a non-scalar, and resource handles await frontend
`foreign import data`.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix),
primarily BE-11 and BE-17..BE-20.

## Scope and dependencies

Complete the reclaiming `cabi_realloc` allocator, the heap-state region, the
four buffer ownership classes, guest-side buffer free after boundary calls,
export `post-return`, and allocator provenance for dynamic stores. The byte
operations, pointer width, and static extent algorithm are owned by
[linear memory and canonical ABI](linear-memory-and-canonical-abi.md); the
canonical adaptation and handle representation are owned by
[canonical ABI and WIT](../../design/backend/wasm/canonical-abi-and-wit.md).

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| ALC-01 | `cabi_realloc` implements the full canonical `(old_ptr, old_len, align, new_len)` contract: allocate when `old_ptr == 0`, free and return `0` when `new_len == 0`, resize with copy otherwise; the returned pointer is aligned to `align`. | Allocator tests for allocate, free, resize, and alignment under Wasmtime. | Verified |
| ALC-02 | Freed blocks are returned to an address-ordered free list, coalesced with adjacent free blocks, and reused; adjacent live/free blocks never overlap and block sizes tile the region. | Allocator reuse/coalescing tests observing a stable `memory.size` across repeated reuse. | Verified |
| ALC-03 | Allocation grows memory with `memory.grow`; a failed grow and any representable-address overflow trap before allocator state is committed. | Grow-failure and overflow trap tests, plus bad-alignment and mismatched-length traps. | Verified |
| ALC-04 | Call-local buffers — indirect parameter records and string transcode buffers — are freed when the canonical call returns. | MIR lowering plus execution evidence that repeated indirect/string calls do not grow linear memory. | Verified |
| ALC-05 | Import-result buffers are freed after their bytes are copied into a fresh GC value. | Result-recovery lowering plus an execution test that repeatedly returns a string without growth. | Verified |
| ALC-06 | A guest export whose lift needs linear memory gets a synthesized `cabi_post_<name>` that frees its return area and owned buffers. | Post-return synthesis test, or the precise blocker when no export has a non-scalar result. | Blocked |
| ALC-07 | A dynamic MIR store is admitted only when its address is proven to come from `cabi_realloc`; other dynamic stores are rejected. | Extent fixtures for allocator-proven and unproven dynamic stores, and the heap-state region. | Verified |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: the `backend/gc-string` worktree on top of `5b331c4`.
Runtime: `wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`.

```text
ALC-01..ALC-03:
  Implementation: crates/psrs-backend/src/wasm/lower/realloc/mod.rs (contract,
    entry, block orchestration) and realloc/block.rs (allocate, reuse/split,
    free/coalesce, bump, growth and overflow traps); the shared
    crates/psrs-backend/src/wasm/lower/asm.rs builder; address constants in
    crates/psrs-backend/src/abi/mod.rs.
  Tests: wasm::lower::realloc::tests::
    realloc_reclaims_reuses_coalesces_aligns_and_traps (sub-checks
    check_reuse_and_coalesce, check_alignment_and_resize, bad_alignment,
    null_with_length, mismatched_length, grow_failure, address_overflow) and
    check_bounded_growth (10,000 alloc/free cycles keep memory.size at one
    page).
  Input boundary: synthesized core module executed under Wasmtime.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend wasm::lower::realloc.
  Result: pass; freed blocks are reused and coalesce, resizes copy and align,
    and every trap path (bad alignment, null with length, mismatched length,
    growth failure, address overflow) traps.
  Gaps: none.
```

```text
ALC-04/ALC-05:
  Implementation: crates/psrs-backend/src/mir/wit/mod.rs (PendingFree,
    free_buffer, import-result free, call-local free emission in reverse order);
    crates/psrs-backend/src/mir/wit/parameters/mod.rs (string transcode buffer);
    crates/psrs-backend/src/mir/wit/parameters/indirect.rs (parameter record).
  Tests: mir::wit::tests::buffers::{list_results_free_the_import_buffer_after_decoding,
    string_arguments_free_the_transcode_buffer_after_the_call};
    psrs-driver tests::wasi::{keeps_multiple_returned_wit_strings_in_distinct_allocations,
    passes_a_returned_wit_string_to_another_import,
    lowers_a_list_returning_import_with_an_allocator,
    lowers_string_log_to_wasi_stdout};
    mir::indirect_tests::*.
  Input boundary: verified MIR; executed components.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass; the boundary lowering ends with `cabi_realloc(ptr, len, align, 0)`
    for each transient buffer, and the executed import string/list and indirect
    parameter cases complete without trapping.
  Gaps: the no-growth claim is execution evidence from the allocator bounded-growth
    check plus the ABI cases; there is no direct post-exit `memory.size` readback
    from a running component.
```

```text
ALC-07:
  Implementation: crates/psrs-backend/src/wasm/lower/extent/mod.rs (scratch and
    heap-state regions), extent/access.rs (allocator-proven dynamic stores),
    extent/address.rs (`Allocated(size)` from a `cabi_realloc` call).
  Tests: wasm::lower::extent::tests::* (allows_reads_and_writes_inside_the_scratch_region,
    allows_reads_and_writes_inside_the_heap_state_region,
    rejects_accesses_that_cross_the_scratch_boundary,
    rejects_accesses_that_start_in_an_unmapped_gap,
    allows_dynamic_reads_but_rejects_dynamic_stores_without_proof).
  Input boundary: verified MIR with memory operations.
  Commands: cargo test -p psrs-backend wasm::lower::extent.
  Result: pass; the heap-state region is modeled, an allocator-derived dynamic
    store fits its `Allocated(size)`, and an unproven dynamic store is rejected.
  Gaps: only the fixed `offset + width <= size` bound is proven for an allocator
    pointer; arbitrary interior arithmetic is not.
```

## Remaining work and blockers

- ALC-06: the only current export, `wasi:cli/run`, returns no aggregate, so
  there is no non-scalar export to attach a `post-return` to. Synthesis is
  blocked until a list-returning export exists (the `SourceType::Array` /
  aggregate source-type work tracked by ABI-08 and BE-19).
- Resource handles (`own<T>` drop and `borrow<T>` release) await the frontend
  accepting `foreign import data`, which is currently rejected at P2.
