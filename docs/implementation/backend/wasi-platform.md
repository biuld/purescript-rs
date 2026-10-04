# WASI Platform Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [WASI platform library](../../design/backend/wasm/wasi-platform-library.md).

**Progress:** Re-baselined by
[DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md) for the guest
encoding, on top of
[DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md) for buffer
reclamation. The executed string path linearizes a GC `(array (mut i8))` of
canonical UTF-8 without transcoding, `list<u8>` is `Array Int`, and the
console and file writes reach the ABI through `stringToBytes`.
WASI-01, WASI-02, WASI-03, WASI-04, WASI-05, WASI-06,
WASI-09, and WASI-10 are Verified. Stdout, stderr, and random bytes execute
under the GC-string representation, and the standard library loads from
`stdlib/lib`. WASI-07 is Verified: `WASI.Process.arguments` and
`WASI.Process.environment` wrap `wasi:cli/environment`, and `WASI.FileSystem`
wraps `wasi:filesystem` with an executed file round-trip and a directory walk.
WASI-08 is In progress: `WASI.Network` wraps the socket services and the wrapper
surface lowers, but no socket execution test is written. The broader BE-22 row
is Partial, BE-23 is Planned, and HTTP/TLS stay Planned/Excluded.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21..BE-23, BE-26.

## Scope and dependencies

Complete componentization, the command entry/exit path, and the enabled WASI
services (console, streams/poll/error, filesystem, sockets, clocks, random, and
the process arguments/environment/exit), plus independent per-service capability
gating. HTTP and TLS
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
| WASI-07 | Filesystem, arguments, and environment services. | `WASI.Process.arguments :: Effect (Array String)` and `WASI.Process.environment :: Effect (Array { _1 :: String, _2 :: String })` wrap `wasi:cli/environment`; `WASI.FileSystem` wraps `wasi:filesystem` (`preopens`, `openRead`/`openWrite`, `readString`/`writeString`, `stat`/`statAt`, `readDirectory`). A file round-trip, a directory walk, and an environment-variable read execute under Wasmtime. | Verified |
| WASI-08 | Sockets, HTTP, and TLS services. | `WASI.Network` wraps `wasi:sockets` (`instanceNetwork`, `createTcpSocket`/`createUdpSocket`, the `tcp*`/`udp*` operations, and the `drop*` helpers); the wrapper surface lowers and validates. HTTP and TLS are outside the synchronous target. | In progress |
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
    library. Each call linearizes a GC `(array (mut i8))` string of canonical
    UTF-8 through `stringToBytes`.
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
  Implementation: stdlib/lib/WASI/Process.purs wraps
    `wasi:cli/environment#get-arguments` as `arguments :: Effect (Array String)`
    and `wasi:cli/environment#get-environment` as
    `environment :: Effect (Array { _1 :: String, _2 :: String })`.
    stdlib/lib/WASI/FileSystem.purs wraps `wasi:filesystem`. The `list<string>`,
    list-of-record, and `option` results use the ABI-08 aggregate paths.
  Tests: psrs-driver tests::wasi::{
    lowers_the_environment_arguments_wrapper_to_an_array,
    reads_environment_arguments_when_wasmtime_is_available,
    reads_environment_variables_as_records_when_wasmtime_is_available,
    rejects_an_import_of_unexported_get_arguments};
    tests::wasi::wrappers::lowers_the_filesystem_wrapper_surface;
    tests::wasi::filesystem::{
    writes_and_reads_a_file_through_preopens_when_wasmtime_is_available,
    reports_a_unit_success_result_as_left_when_wasmtime_is_available,
    stats_and_reads_a_directory_through_preopens_when_wasmtime_is_available};
    psrs-driver tests::module_loader::
    loads_the_standard_library_from_disk_in_trusted_order.
  Input boundary: source and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: pass under Wasmtime 49.0.1. `arguments` recovers the host's
    `list<string>` into a GC array; `environment` recovers a list of records;
    the filesystem round-trip writes, reads back, and walks a directory, and a
    failing unit-success operation reports `Left` without trapping.
  Gaps: none for the wrapped services.
```

```text
WASI-08:
  Implementation: stdlib/lib/WASI/Network.purs wraps `wasi:sockets`
    (`instance-network`, `tcp`, `udp`, and the create-socket interfaces). The
    address and datagram shapes use the ABI-08 aggregate paths.
  Tests: psrs-driver tests::wasi::wrappers::lowers_the_sockets_wrapper_surface
    (every socket wrapper lowers and the artifact validates).
  Input boundary: source; executed component for the non-socket services only.
  Commands: cargo test -p psrs-driver --lib lowers_the_sockets_wrapper_surface.
  Result: pass. No socket execution test is written because the host's network
    capability is not part of the default test profile.
  Gaps: a socket execution test under a network-enabled profile; HTTP/TLS.
    Resumption: add `run_wasmtime_with_sockets` and a loopback round-trip.
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
  Implementation: stdlib/lib/{Prelude.purs,Data/Function.purs,Data/Semigroup.purs,Effect.purs,Effect/Console.purs,Test/Assert.purs,Data/Maybe.purs,Data/Either.purs,WASI/Resource.purs,WASI/IO.purs,WASI/Clock.purs,WASI/Random.purs,WASI/Console.purs,WASI/Process.purs,WASI/FileSystem.purs,WASI/Network.purs,WASI.purs}
    read at runtime by crates/psrs-driver/src/prelude.rs. stdlib/lib/trusted
    lists those modules in trusted-prefix order. User discovery in
    crates/psrs-driver/src/loader.rs still skips those module names.
  Tests: psrs-driver tests::module_loader::
    loads_the_standard_library_from_disk_in_trusted_order,
    does_not_discover_a_user_module_shadowing_the_standard_library;
    tests::library_types::runs_library_maybe_and_either_by_casing_on_just_and_left;
    tests::effects::{run_effect_is_only_available_from_the_selected_entry,
    an_untrusted_prelude_effect_remains_an_ordinary_user_type,
    transitive_effect_types_keep_their_closure_representation};
    tests::data_function::{the_application_operators_resolve_through_the_library_re_export,
    dollar_is_right_associative_and_lowest_precedence};
    tests::semigroup::{array_append_is_a_core_expression,
    the_semigroup_operator_concatenates_strings_and_arrays,
    the_semigroup_operator_is_right_associative,
    a_type_without_an_instance_is_rejected};
    tests::assertions::{the_unit_value_is_a_core_expression_rather_than_an_integer_literal,
    the_unit_value_executes_when_wasmtime_is_available,
    a_held_assertion_lets_the_program_finish_when_wasmtime_is_available,
    a_failed_assertion_traps_with_its_message_when_wasmtime_is_available,
    a_failed_assertion_writes_the_message_before_it_traps,
    a_statement_after_a_failed_assertion_never_runs,
    assert_true_and_assert_false_report_the_value_that_did_not_hold,
    log_show_writes_the_library_rendering};
    tests::wasi::{prints_hello_world_when_wasmtime_is_available,
    reads_the_monotonic_clock_when_wasmtime_is_available,
    rejects_an_import_of_unexported_exit_with_code_raw,
    stored_exit_with_code_leaves_exit_with_code_inside_the_effect_closure,
    lowers_the_environment_arguments_wrapper_to_an_array,
    reads_environment_arguments_when_wasmtime_is_available}.
  Input boundary: standard-library files on disk, plus user source; executed
    component for the console, clock, and assertion cases. The exit wrapper is
    compile/WAT only.
  Commands: cargo test -p psrs-driver --lib standard_library;
    cargo test -p psrs-driver --lib tests::effects;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::semigroup;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::assertions;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi;
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Gaps: none. The loaded set is Prelude, Data.Function, Data.Semigroup, Effect,
    Effect.Console, Test.Assert, Data.Maybe, Data.Either, WASI.Resource, WASI.IO,
    WASI.Clock, WASI.Random, WASI.Console, WASI.Process, WASI.FileSystem,
    WASI.Network, and the WASI umbrella. Data.Maybe, Data.Either, and
    Data.Semigroup are ordinary library declarations and are not in the trusted
    Effect name list. The service modules use Effect from
    Prelude. Their wrappers are effect lambdas, so they are part of the trusted
    Effect representation. The exit evidence does not execute `exitWithCode`.
    `Test.Assert` exports only the four checks that need no class surface; its
    `assertEqual` and `assertThrows` obligations are recorded under EF-12 and
    D-04's M7 section.
```

## Remaining work and blockers

WASI-08 remains In progress: `WASI.Network` wraps and lowers the socket
services, but no socket execution test exists yet. WASI-10 is verified: the
standard library is loaded from `stdlib/lib` rather than embedded sources.
