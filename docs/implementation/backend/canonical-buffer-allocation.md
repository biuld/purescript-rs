# Canonical Buffer Allocation and Lifetime Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Canonical buffer allocation and lifetime](../../design/backend/wasm/canonical-buffer-allocation-and-lifetime.md),
with [DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md).

**Progress:** Split from
[linear memory and canonical ABI](linear-memory-and-canonical-abi.md) so that
buffer allocation and lifetime are tracked on their own. `cabi_realloc` is
still a bump allocator, transient buffers are not yet freed, and no
`post-return` is synthesized.

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
| ALC-01 | `cabi_realloc` implements the full canonical `(old_ptr, old_len, align, new_len)` contract: allocate when `old_ptr == 0`, free and return `0` when `new_len == 0`, resize with copy otherwise; the returned pointer is aligned to `align`. | Allocator tests for allocate, free, resize, and alignment under Wasmtime. | In progress |
| ALC-02 | Freed blocks are returned to an address-ordered free list, coalesced with adjacent free blocks, and reused; adjacent live/free blocks never overlap and block sizes tile the region. | Allocator reuse/coalescing tests observing a stable `memory.size` across repeated reuse. | In progress |
| ALC-03 | Allocation grows memory with `memory.grow`; a failed grow and any representable-address overflow trap before allocator state is committed. | Grow-failure and overflow trap tests, plus bad-alignment and mismatched-length traps. | In progress |
| ALC-04 | Call-local buffers — indirect parameter records and string transcode buffers — are freed when the canonical call returns. | MIR lowering plus execution evidence that repeated indirect/string calls do not grow linear memory. | Unverified |
| ALC-05 | Import-result buffers are freed after their bytes are copied into a fresh GC value. | Result-recovery lowering plus an execution test that repeatedly returns a string without growth. | Unverified |
| ALC-06 | A guest export whose lift needs linear memory gets a synthesized `cabi_post_<name>` that frees its return area and owned buffers. | Post-return synthesis test, or the precise blocker when no export has a non-scalar result. | Blocked |
| ALC-07 | A dynamic MIR store is admitted only when its address is proven to come from `cabi_realloc`; other dynamic stores are rejected. | Extent fixtures for allocator-proven and unproven dynamic stores, and the heap-state region. | In progress |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: the `backend/gc-string` worktree on top of `4826c36`.
Runtime: `wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`.

```text
ALC-01..ALC-03, ALC-07:
  Implementation: crates/psrs-backend/src/wasm/lower/realloc/ (aligned
    allocator), crates/psrs-backend/src/abi/mod.rs (address constants),
    crates/psrs-backend/src/wasm/lower/extent/.
  Tests: pending.
  Input boundary: synthesized core module; verified MIR.
  Commands: pending.
  Result: pending.
  Gaps: bump allocator remains.
```

## Remaining work and blockers

- ALC-01..ALC-03: replace the bump allocator with the reclaiming allocator and
  update the allocator execution suite.
- ALC-04/ALC-05: free string transcode, parameter-record, and import-result
  buffers in `mir/wit`; add no-growth execution evidence.
- ALC-06: the only current export, `wasi:cli/run`, returns no aggregate, so
  there is no non-scalar export to attach a `post-return` to. Synthesis is
  blocked until a list-returning export exists (the `SourceType::Array` /
  aggregate source-type work tracked by ABI-08 and BE-19).
- ALC-07: add the heap-state region to the extent regions and fixtures.
