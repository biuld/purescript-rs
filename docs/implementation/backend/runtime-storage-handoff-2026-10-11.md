# Runtime and ordinary storage binding handoff

Date: 2026-10-11. Repository: `/Users/biu/Projects/purescript-rs`.
Branch: `stdlib/vendor-core-libraries`. Baseline HEAD: `e806616e` (`Remove
compiler-owned Effect and State mechanisms`). The work described below was
subsequently committed as three topical commits (`395ccba6`, `e7e250cf`,
`981c832a`), with this record following. No push or PR was created.

## Direction and boundaries

The user requested deletion of compiler-owned State analysis and the legacy
Effect mechanism, then a neutral algebraic-effects design, runtime cleanup,
and repair of ordinary runtime storage bindings. Do not restore State tokens,
dependency chains, effect-specific pass rules or the legacy Effect entry path.
The user has not selected an ordinary thunk replacement.

Algebraic effects and Effect/ST are design drafts. Their eventual handlers and
sequencing belong to library/runtime code with ordinary compiler contracts.
DEC-03 governs library effects; runtime delimited continuations require their
own reviewed control contract and are not implemented here.

## Runtime changes

- `crates/psrs-runtime/Cargo.toml` and `src/lib.rs`: only `catalog` is enabled by
  default; `formatter` was renamed to `number`. Catalog/GC encoding/packaging
  dependencies are host-only. Executable features are additive; pinned artifact
  builds still select separate feature sets and storage contracts.
- The old `examples/package.rs` is deleted; the same packaging operation is now
  an explicit `psrs-runtime-package` binary in `tools/package.rs`, gated by
  `artifact-tool`. Include the untracked new file when reviewing/committing.
- `tools/build.sh` and `tools/build-allocator.sh` use separate target directories
  (`runtime-number`, `runtime-allocator`) under `${CARGO_TARGET_DIR:-target}`.
- `src/allocator/algorithm.rs` and `wasm.rs` use unsigned wasm32 operands. Live
  addresses above 2 GiB no longer fail signed-pointer validation. Rust backing
  layouts above `isize::MAX` fail allocation; full-memory byte coverage saturates
  rather than wrapping. This does not promise a single allocation above 2 GiB.
- The allocator artifact and its catalog digest were repinned:
  `4eca097da7112522a23cd790d7562d643090ac7d5f63aa493e853b23925dfb23`.
  The numeric artifact is unchanged. ABI/storage/stack upper-bound verification
  passed through composition. The declared allocator stack bound remains 80 bytes.
- `src/abi/storage/{mod,projection}.rs` no longer describes State operands.
- `crates/psrs-linker/tests/heap_getter.rs` exercises real high-bit addresses with
  a heap boundary of `0x80000000`, including content-preserving resize and free.
- `crates/psrs-driver/src/tests/show.rs` updates its expected allocator digest
  to match this repin. That stdlib-dependent test was not executed in this work.

## Ordinary storage binding repair

Before this repair, three backend boundaries unconditionally rejected runtime
storage declarations with `runtime storage bindings require the withdrawn State
contract`. This baseline was established by inspecting the current checkout,
not by an independent before-build comparison.

Implemented source contracts:

```purescript
foreign import "psrs:runtime-storage#array_fill"
  filled :: forall a. Int -> a -> Array a
foreign import "psrs:runtime-storage#array_read"
  read :: forall a. Array a -> Int -> a
foreign import "psrs:runtime-storage#array_write"
  write :: forall a. Array a -> Int -> a -> Unit
foreign import "psrs:runtime-storage#trap"
  abort :: forall a. a
```

Concrete scalar/array signatures are also exercised. Function payloads are
exercised through polymorphic declarations. Calling boundaries still follow the
existing Core call decomposition; no new source syntax or function-result arity
model was introduced.

Ownership and changed files:

1. `bindings/runtime/mod.rs` and new `source.rs` validate every checked Core
   signature, including unused imports. The runtime owns operand categories;
   Core's `types_equivalent` checks agreement of array elements and payloads.
   Missing source signatures, wrong arity/types and mismatched elements fail.
2. `bindings/runtime/call/mod.rs` canonicalizes element slots to the existing CC
   erased payload protocol and verifies incoming CC bindings. New `call/tests.rs`
   rejects missing signatures, wrong arity/result and invalid array storage.
   The normal CC call/partial-application conversion machinery performs payload
   boxing, recovery and callable adaptation. Arrays must keep their canonical
   erased-element storage and identity; do not add element-wise copy adaptation
   for mutation.
3. `bindings/runtime/target.rs` plans physical imports from the provider ABI and
   application layout; `mir/layout/accessors.rs` exposes read-only type definitions
   for physical contract verification. Provider identity survives into MIR.
4. `mir/runtime/context.rs` now retains live imports instead of an empty map.
   `mir/{mod,lower/mod,lower/assignments}.rs` pass this inventory to ordinary call
   lowering. New `mir/lower/storage_call.rs` handles nullable raw references,
   checked recovery, void-write Unit and non-returning trap results. WIT builder
   and its test receive the inventory too. This adds no execution-order analysis.
5. `cc/lower/global.rs` looks up an external's checked scheme for zero-operand
   polymorphic values, rather than requiring a local declaration. This enables
   the ordinary polymorphic non-returning import; it is a general external-value
   correction.
6. New `crates/psrs-driver/src/tests/runtime_storage.rs` covers the complete
   source-to-component path without requiring the independent stdlib package.

## Validation and limits

Successful focused checks:

| Check | Evidence |
| --- | --- |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-runtime --all-features --lib` | 26 passed, including two unsigned allocator cases and runtime GC provider execution |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-linker --test heap_getter` | 4 passed with the repinned artifact; the execution test was then rerun successfully with the added high boundary |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend --lib post_return::` | 3 passed, including composed buffer reclamation |
| `cargo check --locked -p psrs-runtime --lib --target wasm32-unknown-unknown --all-features` | Passed |
| Concurrent numeric/allocator build scripts and `cmp` against pinned artifacts | Both reproduced byte-for-byte |
| `cargo clippy -p psrs-runtime --all-targets --all-features -- -D warnings` | Passed |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib runtime_storage::` | 6 passed; aliases/partial calls/concrete and generic payloads, invalid declarations, traps and unused-provider omission |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend --lib runtime -- --nocapture` | 11 passed; malformed IR and provider/linking/execution checks |
| `cargo clippy -p psrs-backend -p psrs-driver --all-targets -- -D warnings` | Passed |

Additional erasure regression was attempted:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit::
```

Result: 1 passed, 20 failed. All failures reported that
`/Users/biu/Projects/psrs-stdlib` content differs from `stdlib.lock.json`, before
backend lowering. This is an environment/package admission failure, not evidence
of a backend regression or a successful broad regression run. One linked-module
high-bit integer case passed because it uses explicit sources. Do not change the
lock or point these tests at the State-dependent development package merely to
bypass this failure; provide the actual locked package for comparable evidence.

Final `cargo fmt --all --check` and `git diff --check` passed after the storage
continuation's formatting correction. Runtime script syntax checks also passed.
No full workspace tests, full clippy,
official scoreboards, source-fidelity audit or public Effect/ST conformance run
was performed. Existing user preference is focused validation, not repeatedly
running the full workspace suite.

## Documents and independent package

Relevant normative and acceptance documents updated in this workspace:

- `docs/design/backend/wasm/linking-and-runtime.md`
- `docs/design/backend/wasm/canonical-realloc-runtime-adapter.md`
- `docs/implementation/backend/linking-and-runtime.md`

Other draft design work landed with the documentation commit:

- `docs/design/backend/fp/algebraic-effects-and-handlers.md`
- `docs/implementation/backend/algebraic-effects.md`
- `docs/design/backend/fp/native-effect-and-st-handlers.md`
- `docs/implementation/backend/native-effect-and-st.md`
- Backend README index and `docs/feature/F-02-portable-programs.md` changes.

The algebraic/native-effect checklists remain Unverified. Storage ABI acceptance
does not implement either topic. `/Users/biu/Projects/psrs-stdlib` was not edited
in this work. Its earlier handoff reported substantial uncommitted State-based
adaptations; inspect that repository before any migration. Compiler
`stdlib.lock.json` remains unchanged and points at the old locked package.

## Completion

1. The diff and untracked source files were reviewed. The draft design work and
   the independently committed State-removal baseline were preserved.
2. The shared zero-operand external lowering change was reviewed; focused storage
   behavior and the final formatting/diff checks are green.
3. Broader erasure evidence was measured. The locked revision `1e01310`,
   materialized without modifying the lock or the development repository,
   reproduces fingerprint `fnv1a64-v1:bca0ace40bd0ccac` and yields `9 passed,
   12 failed` for `polymorphism_erasure_audit`. The same 12 tests fail at
   baseline `e806616e`, so the storage repair changes none of them. Nine are the
   locked `Prelude`'s removed `psrs:effect` imports at P8 WIT linking; three are
   pre-existing `guest projection of a non-closed type` closure-conversion
   cases. The committed lock is stale relative to the effect removal, so a green
   cohort needs a matching compiler/stdlib pair.
4. Runtime cleanup and ordinary storage binding repair were committed as
   coherent topics with their docs and tests, including the renamed packaging
   tool and the new backend/driver files.
5. Algebraic-effect and native Effect/ST implementation remains a separate,
   explicitly selected next task. Library compatibility is not claimed and the
   normal stdlib lock was not updated.

