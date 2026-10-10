# Checked scalar foreign bindings

This iteration follows the [source restoration checkpoint](../vendor-restoration-2026-10-06/report.md)
at `f0b1b71`. It implements 20 previously missing foreign values through explicit
`psrs:intrinsic` bindings. All official public signatures and ordinary source
bodies remain unchanged; only the target binding string is inserted into those
foreign declarations. The [binding manifest](bindings.json) records each
operation, exact signature, and pinned upstream declaration.

## Implementation contract

The compiler resolves the explicit operation through the existing authoritative
intrinsic registry. It retains the declaring symbol and checked source type.
P8 constructs typed Core functions over those parameters, validates them with
the common Core intrinsic rules, and transfers the declaration identity only
after validation. Ordinary closure conversion handles first-class functions,
partial applications, and imported fixity aliases. No source function name or
module name selects an implementation.

The input Core is verified before traversing the signature; cyclic or missing
type evidence is invalid IR. Linking uses a complete candidate and does not
publish a previously generated binding if another binding fails. A declaration
with the wrong argument or result type fails even when unused. Non-scalar
categories remain explicit unsupported bindings; they are not approximated.

## Source and target evidence

| Official module | Implemented foreign values |
| --- | --- |
| `Data.Int` | toNumber |
| `Data.Int.Bits` | and, or, xor, shl, shr, zshr, complement |
| `Data.Eq` | eqBooleanImpl, eqIntImpl, eqNumberImpl, eqCharImpl |
| `Data.Ring` | intSub, numSub |
| `Data.Semiring` | intAdd, numAdd, numMul |
| `Data.HeytingAlgebra` | boolConj, boolDisj, boolNot |

Wasm uses signed i32 for Int and IEEE-754 f64 for Number, as specified by
[scalars and primitives](../../../design/backend/fp/scalars-and-primitives.md).
DEC-16 supplies the Char scalar-value contract. Numeric conversion of every
i32 to f64 is exact; it replaces the missing foreign implementation without
inventing a source equation.

The oracle generator executes the original JavaScript from the exact clean
package revisions pinned in the restoration inventory. It records JavaScript
file hashes and both official and target observations. The committed
[fixtures](../../../../crates/psrs-driver/tests/fixtures/stdlib-scalar/observations.json)
cover 20 bindings in 46 cases, including integer wrapping, shift-count masking,
negative values, NaN, infinities, signed zero, and non-ASCII/supplementary Char.
Node v26.10.0 produced these observations; Wasmtime 49.0.2 executed the target.

Two cases deliberately differ in representation: official JS evaluates
`zshr(-1, 0)` and `zshr(-1, 32)` to `4294967295`; this target returns `-1`, the
same 32-bit bit pattern interpreted as the specified signed Int. Both raw and
normalized results remain visible. This is a Wasm Int representation difference,
not an assertion of identical JavaScript number behavior. The source signature
and shift-count masking are preserved.

The large-product behavior of official `intMul`, division/remainder edge cases,
string equality, callback-based array equality, and other foreign values were
not silently mapped to superficially similar primitives. They remain explicit
gaps pending their implementation and behavior review.

## Validation and remaining work

Mandatory Wasmtime driver tests cover the 46-case official observation fixture,
the complete actual vendored Bits module, first-class and partially applied
functions, imported operator identity, unused wrong signatures, unknown
operations, and unimplemented operation categories. The fixture test also checks
that every tested declaration is still the actual vendored declaration.

Seven primitive-foreign driver tests passed with `PSRS_REQUIRE_WASMTIME=1`.
Fourteen let-constraint tests and six ordinary-library-foreign tests passed.
Backend tests cover transactional failure, checked input cycles, identity
transfer, and verification of the generated function; all three passed. Format
checking and strict workspace clippy with all targets also passed. The full
workspace test suite and full scoreboards were not run.

A 46-level right-nested fixture originally overflowed the Rust test thread's
stack. The generator now combines the same independent conditions as a
balanced tree; no case was removed. Arbitrarily deep expression robustness was
not established by this iteration.

After rebuilding the CLI, `/tmp/psrs-stdlib-all.purs` still fails at P8 library
linking. The missing-implementation diagnostics decreased from 253 to 233, with
`Control.Apply.arrayApply` still first. The report is
`/tmp/psrs-stdlib-primitive2.json`; the observed run took 43,198 ms. Twenty of the
275 inventory foreign values now have target implementations, leaving 255 in
the full inventory, including modules outside this reproducer's closure.
Full stdlib compilation and full runtime acceptance remain incomplete. No
scoreboard, README, or D-04 measurement was changed.

The fixtures isolate these foreign declarations from other missing imports;
they do not establish runtime behavior of the complete Eq/Semiring/Prelude
closures. The Bits runtime test imports the actual complete module. A future
standalone conformance component must keep this distinction visible when
moving the oracle generator and fixture runner out of compiler tests.

## Reproduce

```sh
node docs/workflow/tools/stdlib-scalar-oracle.mjs \
  /tmp/purescript-prelude /tmp/ps-pkgs/purescript-integers \
  crates/psrs-driver/tests/fixtures/stdlib-scalar
PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --lib primitive_foreign --offline
cargo test -p psrs-backend --lib bindings::primitives --offline
```

The source-fidelity policy still applies. This manifest covers target bindings;
it does not authorize edits to ordinary PureScript bodies or signatures.
