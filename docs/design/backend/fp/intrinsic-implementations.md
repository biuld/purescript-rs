# Intrinsic semantic and target implementation contracts

**Feature:** [F-02](../../../feature/F-02-portable-programs.md)

**Related design:** [IR boundaries](../../D-01-frontend-and-ir-boundaries.md),
[scalars](scalars-and-primitives.md), and
[target linking](../wasm/linking-and-runtime.md).

## Semantic identity

HIR owns stable intrinsic identities, language schemes, arity, and conservative
semantic effects. Names describe language values: IntAdd, IntQuot, IntRem,
NumberAbs, and StringToBytes. Machine widths and signed opcodes belong to MIR,
where IntAdd selects I32Add and NumberAbs selects F64Abs. Renaming a language
operation must preserve its symbol ID. Retired IDs remain reserved and cannot
be reused or bootstrapped as callable operations.

An intrinsic need not correspond to one machine opcode. Opaque representation
operations and correctly rounded numerical conversion are valid primitive
boundaries. Library algorithms and policies remain ordinary library functions.
Integer Euclidean div/mod, including negative-divisor and zero-divisor policy,
belong to psrs-stdlib over checked truncating IntQuot/IntRem. The former compiler
IntDiv/IntMod floor algorithms are retired, not aliases for the official API.
Their IDs are reserved. The official source functions and bindings remain intact.

The vocabulary also includes values and compile-time operations. Checked
coercions and unsupported partial values must be classified explicitly; they
must not fall through to a scalar emitter or acquire a fabricated runtime body.

## Target selection

The backend owns one exhaustive implementation selection keyed by intrinsic
identity. A supported runtime operation selects one of:

- Direct: a CC scalar operation selecting a MIR opcode or instruction sequence.
- Generated: a representation operation lowered to checked local IR, including
  storage operations, codecs and explicit conversion plans.
- Artifact: a pinned executable export with a raw signature and value protocol.

Compile-time operations and unsupported values have separate explicit cases.
HIR must not depend on the backend, runtime code, or raw target ABI. Runtime
metadata must not depend on compiler IR. The backend performs the conversion
between language contracts, CC shapes, MIR values and the runtime catalog.

Adding a supported intrinsic requires an implementation case. Missing cases
must fail compilation or report unsupported behavior, never panic in a generic
scalar fallback. Operation effects have one semantic owner; optimization must
retain possible traps and mutations independently of the selected provider.

## Artifact calls

CC retains an intrinsic identity and checked arguments for an artifact call.
Before erasure, the verifier checks operand and result shapes against the
intrinsic's closed language scheme. MIR consumes the selected value protocol:
raw scalars, a borrowed canonical UTF-8 input buffer, or caller-owned bounded
UTF-8 output. Protocol metadata records capacity and normal-return release.
Existing reviewed codecs implement representation conversion; the linker does
not reconstruct language layouts. Unsupported schemes or protocols are errors.

The import signature in actual MIR is the consumer's contract. Target planning
must compare it with the selected export ABI, then verify the export against
real artifact bytes. Setting both expected and provided signatures from the
catalog does not establish compatibility with the consumer. Arity, operand
width, result type, and GC references require rejection tests at this boundary.

Generated bindings must likewise satisfy the signature of the generated body;
reserved symbols identify a provider, not evidence that an arbitrary signature
is correct. Wasm emission consumes the checked selection and memory plan.

Artifact provenance, digest, initialization, private storage, memory ownership,
and reachable stack bounds remain governed by the target-linking design. A trap
aborts the command; successful raw calls release transient buffers and retain
no pointer. A direct implementation contributes no artifact dependency.

## Acceptance

Verify stable IDs and reserved slots, exhaustive target selection, checked
source shapes, malformed MIR imports, and actual export contracts. Execute
direct operations and artifact calls, including unsaturated source uses,
signed zeros, nonfinite values, retained strings and buffer reuse. Compare
public integer div/mod against pinned official JS on all sign combinations,
zero divisors and representable bounds. Unrepresentable quotient overflow
remains an explicit Wasm trap. Retired floor bindings must be unavailable.
