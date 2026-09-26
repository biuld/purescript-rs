# WASI Platform Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [WASI platform library](../../design/backend/wasm/wasi-platform-library.md).

**Progress:** WASI-01 through WASI-06 and WASI-09 Verified. WASI-07 (filesystem,
arguments, environment), WASI-08 (sockets/HTTP/TLS), and WASI-10 (loading the
standard library from disk) are not implemented. The broader BE-22 row is
Partial, BE-23 is Planned, and the excluded services stay Planned/Excluded.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21..BE-23, BE-26.

## Scope and dependencies

Complete componentization, the command entry/exit path, and the implemented WASI
services (console, monotonic clock, random), plus independent per-service
capability gating. Filesystem, arguments, environment, sockets, HTTP, and TLS
are specified but out of the current synchronous target. Module loading and the
embedded standard library are tracked here through BE-26. The canonical ABI
bytes are owned by
[linear memory and canonical ABI](linear-memory-and-canonical-abi.md); the
validator/encoder by [Wasm encoding](wasm-encoding.md).

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| WASI-01 | The core module is componentized into a WASI 0.2 command with UTF-8 strings and matching world. | Component emission and world/capability tests. | Verified |
| WASI-02 | The command entry calls `wasi:cli/run` and exits with the program result. | Executed component returns the program exit code. | Verified |
| WASI-03 | Console stdout and stderr are wired and observable. | Stdout/stderr execution tests and effect ordering. | Verified |
| WASI-04 | Monotonic clock is wired. | Clock execution test. | Verified |
| WASI-05 | Random bytes are wired. | Random execution test. | Verified |
| WASI-06 | Each enabled WASI service package has an independent capability gate; a disabled service fails before lowering. | Per-service gate test plus a disabled-service rejection. | Verified |
| WASI-07 | Filesystem, arguments, and environment services. | Not implemented; capability flags disabled. | In progress |
| WASI-08 | Sockets, HTTP, and TLS services. | Outside the synchronous target; excluded/planned. | In progress |
| WASI-09 | User modules are discovered from the filesystem and the import graph is followed. | Entry files' directories are indexed by module name; imported modules are loaded transitively and executed. | Verified |
| WASI-10 | The standard library is loaded from disk rather than embedded in the driver. | Not implemented; the embedded prelude is still prepended. | In progress |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: `c4e65dd` plus the WASI evidence changes in this worktree. Runtime:
`wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`.

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
    library.
  Tests: component::tests::prints_via_wasi_stdout_when_wasmtime_is_available;
    psrs-driver tests::wasi::{lowers_string_log_to_wasi_stdout,
    prints_hello_world_when_wasmtime_is_available,
    writes_to_stderr_when_wasmtime_is_available};
    tests::effects::bind_preserves_wasi_results_across_stdout_and_stderr.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
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
  Implementation: random import wiring in the ABI registry.
  Tests: psrs-driver tests::wasi::reads_random_bytes_when_wasmtime_is_available.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: pass.
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
  Result: not implemented.
  Gaps: filesystem, arguments, and environment services. Resumption: add the
    source library and aggregate/list lowering, then expose and gate the
    services.
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
  Implementation: the standard library is embedded in
    crates/psrs-driver/src/prelude.rs and prepended to the source list; there
    is no discovery path for it.
  Tests: none.
  Input boundary: n/a.
  Commands: n/a.
  Result: not implemented.
  Gaps: load the standard library from disk like any module and retire the
    embedded sources.
```

## Remaining work and blockers

WASI-07, WASI-08, and WASI-10 remain. WASI-07 (filesystem/arguments/environment)
depends on general aggregate/list ABI coverage (ABI-06); WASI-10 retires the
embedded prelude once the library ships on disk.
