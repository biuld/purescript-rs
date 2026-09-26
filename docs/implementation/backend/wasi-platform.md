# WASI Platform Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [WASI platform library](../../design/backend/wasm/wasi-platform-library.md).

**Progress:** Re-baselined by
[DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md) for GC strings
and buffer reclamation. WASI-01, WASI-02, WASI-03, WASI-04, WASI-05, WASI-06,
WASI-09, and WASI-10 are Verified. Stdout, stderr, and random bytes execute
under the GC-string representation, and the standard library loads from
`stdlib/lib`. WASI-07 (filesystem, arguments, environment) and WASI-08
(sockets/HTTP/TLS) are not implemented. The broader BE-22 row is Partial,
BE-23 is Planned, and the excluded services stay Planned/Excluded.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21..BE-23, BE-26.

## Scope and dependencies

Complete componentization, the command entry/exit path, and the implemented WASI
services (console, monotonic clock, random), plus independent per-service
capability gating. Filesystem, arguments, environment, sockets, HTTP, and TLS
are specified but out of the current synchronous target. Module loading and the
on-disk standard library are tracked here through BE-26. The canonical ABI
bytes are owned by
[linear memory and canonical ABI](linear-memory-and-canonical-abi.md); the
validator/encoder by [Wasm encoding](wasm-encoding.md).

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| WASI-01 | The core module is componentized into a WASI 0.2 command with UTF-8 strings and matching world. | Component emission and world/capability tests. | Verified |
| WASI-02 | The command entry calls `wasi:cli/run` and exits with the program result. | Executed component returns the program exit code. | Verified |
| WASI-03 | Console stdout and stderr are wired and observable, linearizing GC strings per call. | Stdout/stderr execution tests and effect ordering. | Verified |
| WASI-04 | Monotonic clock is wired. | Clock execution test. | Verified |
| WASI-05 | Random bytes are wired, recovering the returned byte list into a GC value. | Random execution test. | Verified |
| WASI-06 | Each enabled WASI service package has an independent capability gate; a disabled service fails before lowering. | Per-service gate test plus a disabled-service rejection. | Verified |
| WASI-07 | Filesystem, arguments, and environment services. | Not implemented; blocked on the general aggregate/list ABI (`list<string>` arguments and results). | Blocked |
| WASI-08 | Sockets, HTTP, and TLS services. | Outside the synchronous target; excluded/planned. | In progress |
| WASI-09 | User modules are discovered from the filesystem and the import graph is followed. | Entry files' directories are indexed by module name; imported modules are loaded transitively and executed. | Verified |
| WASI-10 | The standard library is loaded from disk rather than embedded in the driver. | `stdlib/lib` is read at runtime in trusted-prefix order; existing library and execution tests pass. | Verified |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: `81b2eee` plus the DEC-10 re-verification in this worktree. Runtime:
`wasmtime 49.0.1` under `PSRS_REQUIRE_WASMTIME=1`.

```text
WASI-01:
  Implementation: crates/psrs-backend/src/component.rs.
  Tests: component::tests::{componentizes_a_command_exporting_run,
    component_world_matches_the_capability_profile};
    psrs-driver tests::integration::emits_a_wasi_command_component.
  Input boundary: encoded core module.
  Commands: cargo test -p psrs-backend component;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Gaps: none.
```

```text
WASI-02:
  Implementation: crates/psrs-backend/src/wasm/lower/mod.rs (entry) and
    component.rs.
  Tests: component::tests::runs_the_command_when_wasmtime_is_available;
    psrs-driver tests::wasi::runs_main_as_a_wasi_component_when_wasmtime_is_available.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Gaps: none.
```

```text
WASI-03:
  Implementation: WASI stdout/stderr imports in the ABI registry; effects
    library. Each call linearizes a GC `(array (mut i16))` string to UTF-8.
  Tests: component::tests::prints_via_wasi_stdout_when_wasmtime_is_available;
    psrs-driver tests::wasi::{lowers_string_log_to_wasi_stdout,
    prints_hello_world_when_wasmtime_is_available,
    prints_a_non_ascii_literal_when_wasmtime_is_available,
    prints_an_interned_literal_once_per_use_when_wasmtime_is_available,
    writes_to_stderr_when_wasmtime_is_available};
    tests::effects::bind_preserves_wasi_results_across_stdout_and_stderr.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend --lib
    prints_via_wasi_stdout_when_wasmtime_is_available;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    lowers_string_log_to_wasi_stdout prints_hello_world_when_wasmtime_is_available
    prints_a_non_ascii_literal_when_wasmtime_is_available
    prints_an_interned_literal_once_per_use_when_wasmtime_is_available
    writes_to_stderr_when_wasmtime_is_available
    bind_preserves_wasi_results_across_stdout_and_stderr.
  Result: pass under Wasmtime 49.0.1. `log` prints `hello world`, `héλ`, and a
    repeated interned literal; `error` prints `oops` to stderr; bind runs
    stderr before stdout.
  Gaps: none.
```

```text
WASI-04:
  Implementation: clock import wiring in the ABI registry.
  Tests: psrs-driver tests::wasi::reads_the_monotonic_clock_when_wasmtime_is_available.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: pass.
  Gaps: none.
```

```text
WASI-05:
  Implementation: random import wiring in the ABI registry. A returned byte
    list is decoded into a fresh GC string.
  Tests: psrs-driver tests::wasi::{reads_random_bytes_when_wasmtime_is_available,
    passes_a_returned_wit_string_to_another_import}.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    reads_random_bytes_when_wasmtime_is_available
    passes_a_returned_wit_string_to_another_import.
  Result: pass under Wasmtime 49.0.1. `get-random-bytes` returns, and logging
    that string prints a non-empty UTF-8 payload plus a newline.
  Gaps: none.
```

```text
WASI-06:
  Implementation: crates/psrs-backend/src/abi/ classification/registry
    capability checks.
  Tests: abi::tests::capability_gates::
    current_wasi_service_packages_have_independent_profile_gates;
    abi::tests::rejects_a_disabled_wasi_service_before_lowering;
    psrs-driver tests::wasi::
    rejects_a_wasi_interface_outside_the_component_capability_profile.
  Input boundary: WIT registry and reduced profiles.
  Commands: cargo test -p psrs-backend abi::;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: pass.
  Gaps: none.
```

```text
WASI-07:
  Implementation: WIT descriptions vendored; no source library or aggregate
    lowering yet.
  Tests: none; the capability flags are disabled.
  Input boundary: n/a.
  Commands: n/a.
  Result: blocked.
  Gaps: filesystem, arguments, and environment services. Blocked on ABI-06
    (general aggregate/list coverage, including `list<string>`). Resumption:
    land ABI-06, add the source library, then expose and gate the services.
```

```text
WASI-08:
  Implementation: none.
  Tests: none.
  Input boundary: n/a.
  Commands: n/a.
  Result: excluded from the synchronous target.
  Gaps: sockets/HTTP/TLS. Resumption: separate platform decision and an async
    language/library plan.
```

```text
WASI-09:
  Implementation: crates/psrs-driver/src/loader.rs (`load_program_files`);
    wired through crates/psrs-cli/src/main.rs.
  Tests: psrs-driver tests::module_loader::
    discovers_an_imported_module_from_the_entry_directory (loads `Helper`
    from `Main`'s directory and executes to 42),
    loads_only_the_modules_that_are_imported.
  Input boundary: source files on disk; executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib module_loader.
  Result: pass; the CLI `build` discovers sibling modules without listing them.
  Gaps: none for user-module discovery.
```

```text
WASI-10:
  Implementation: stdlib/lib/{Prelude.purs,WASI/Console.purs,WASI/Clock.purs,WASI/Random.purs}
    read at runtime by crates/psrs-driver/src/prelude.rs. stdlib/lib/trusted
    lists those modules in trusted-prefix order. User discovery in
    crates/psrs-driver/src/loader.rs still skips those module names.
  Tests: psrs-driver tests::module_loader::
    loads_the_standard_library_from_disk_in_trusted_order,
    does_not_discover_a_user_module_shadowing_the_standard_library;
    tests::effects::{run_effect_is_only_available_from_the_selected_entry,
    an_untrusted_prelude_effect_remains_an_ordinary_user_type,
    transitive_effect_types_keep_their_closure_representation};
    tests::wasi::{prints_hello_world_when_wasmtime_is_available,
    reads_the_monotonic_clock_when_wasmtime_is_available}.
  Input boundary: standard-library files on disk, plus user source; executed
    component for the WASI cases.
  Commands: cargo test -p psrs-driver --lib standard_library;
    cargo test -p psrs-driver --lib tests::effects;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi;
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Gaps: none. The loaded set is Prelude, WASI.Console, WASI.Clock, and
    WASI.Random. WASI.Random uses Effect from Prelude and is not part of the
    trusted Effect representation.
```

## Remaining work and blockers

WASI-07 and WASI-08 remain. WASI-07 (filesystem/arguments/environment)
depends on general aggregate/list ABI coverage (ABI-06). WASI-10 is verified:
the standard library is loaded from `stdlib/lib` rather than embedded sources.
