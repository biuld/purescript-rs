# Checked instantiation of bare row arguments

Measured on 2026-10-07.

## Contract and change

Core's checked type relation owns declaration instantiation and the residual
row substitutions consumed by local-row materialization. A row beneath a nominal
constructor, such as `Proxy (y :: Int | r)`, is invariant but still matches by
label. Distinct label order is irrelevant; duplicate bare-row labels retain
their occurrence order. Closed and rigid rows cannot absorb unmatched fields.

Previously Core compared bare `RowExtend` nodes positionally, although record
rows already had a residual relation and P5 accepted the nominal argument.
The inferred nested `Union` use consequently failed at P7 when `y` moved behind
`a` and `b` in the concrete row. Both invariant and subsumption entry points now
delegate bare rows to the shared row relation. Record-specific duplicate checks
stay at the record boundary. Residual field equality also uses the shared
invariant relation, including nested nominal row arguments.

The matcher retains a checked residual even when no arena node represents it.
No verifier check was bypassed, and no Union-specific backend conversion was
added. Tests verify reordered fields, duplicate-label occurrence order,
consistent repeated residuals, nested row fields, rigid-tail rejection,
missing-field rejection, and incompatible field types.

## Evidence

The baseline is HEAD `11db1757c20b76b5231e033395f000714f279baa` with the preceding
WIT payload repair in the working tree. The locked package remains revision
`01d6cd406cdce68a7ea1ec4c26a44793ead34571`, fingerprint
`fnv1a64-v1:b2890fecd9c42aa3`.
`core-row-instantiation-observations.json` records the comparable single-file diagnosis and
the executed Wasm artifact separately.

| Check | Baseline | Result |
| --- | --- | --- |
| Nested Union diagnosis | P7 Core type/context mismatch, span 476–482 | Compilation accepted |
| Nested Union Wasmtime execution | Blocked at Core verification | Exit 42, empty stdout/stderr |
| `prim_row` integration tests | Nested execution failed | 12 passed |
| Core unit and optimizer tests | Existing cases | 87 passed |
| Rank-N source/runtime tests | Existing cases | 2 passed |
| Cross-declaration runtime tests | Existing cases | 9 passed |

The complete fixture is `tests/prim_row/deferred_execution.rs::SOURCE`.
Its inferred constraint requires `Union (y :: Int | left) right output`; `main`
instantiates it with `left = (a :: Int)`, `right = (b :: Boolean)`, and the
three-field output. The test requires Wasmtime when `PSRS_REQUIRE_WASMTIME=1`.

```sh
cargo test -p psrs-core --lib
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test prim_row
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test rank_n
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib declaration_calls::
```

The preceding WIT payload fix is checked separately with the 146 WASI driver
tests. Formatting, source layout, and workspace clippy are integration checks;
the full workspace test suite was omitted by the user's explicit instruction.
