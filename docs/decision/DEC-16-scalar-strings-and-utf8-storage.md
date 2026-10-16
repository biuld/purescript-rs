# DEC-16 — Scalar Strings and UTF-8 Storage

**Status:** Accepted
**Date:** 2026-10-01

## Context and constraints

`purs` represents a source `String` as UTF-16. That choice leaks into every
layer that has to agree on what a string *is*:

- A lone surrogate is a value the language can hold but cannot display. The
  vendored corpus depends on that: `passing/StringEscapes.purs` and the two
  `StringEdgeCases` files contain lone-surrogate escapes that `purs` accepts.
- `Char` is likewise a UTF-16 code unit, so `failing/2434.purs` rejects
  `'\x10000'` and a supplementary scalar is not one `Char`.
- A Wasm GC string can be stored as `(array (mut i16))`, which is what the
  backend did, with a transcoder at the canonical ABI boundary that converts
  UTF-16 to and from the component's UTF-8. The transcoder replaces malformed
  input with U+FFFD, so an invalid sequence is silently repaired rather than
  reported.

The Unicode standard does not require any of that. Text is a sequence of
*scalar values*, surrogates are not scalar values, and UTF-8 is the interchange
encoding every platform this target runs on uses. WebAssembly GC has no
UTF-16 string type: a string is an array of bytes or an array of 16-bit code
units, so storing UTF-16 means choosing the array width and paying for a
transcoder at every boundary.

The constraint that makes this a decision rather than a detail is
[D-13](DEC-13-wit-to-source-type-mapping.md). WIT distinguishes `string` from
`list<u8>`, and the corpus and the WASI world rely on that distinction:
`list<u8>` carries arbitrary bytes, `string` carries text. Mapping both to the
source `String` erases the only distinction the WIT type system draws, and it is
what made the transcoder necessary in the first place.

## Decision

A source `String` is a sequence of Unicode scalar values. It is stored as
canonical UTF-8 and is validated strictly at every boundary.

- **Front end.** String and character literals are normalized to scalar
  sequences. A contiguous escaped surrogate pair (`\xD834\xDF06`) decodes as one
  scalar. An unpaired surrogate escape is a lexical error, not U+FFFD. A
  supplementary scalar is a valid single `Char`.
- **Storage.** A Wasm GC string is `(array (mut i8))` holding canonical UTF-8.
  Length in storage is a byte count. A distinct string literal remains a passive
  data segment materialized once with `array.new_data` and interned in a lazily
  initialized module global.
- **Boundary.** The canonical ABI copies WIT `string` bytes without transcoding
  and validates them strictly. A malformed sequence or an unpaired surrogate
  traps. No path produces U+FFFD.
- **`list<u8>`.** A WIT `list<u8>` is source `Array Int`, distinct from
  `String`. It is copied element by element, each element range-checked against
  `0..255` before it is narrowed to one byte, and it is never decoded as text.
  An out-of-range element traps.
- **Scalar-value boundaries.** Any source-visible string operation that counts,
  indexes, or slices works on scalar values, not code units or bytes. Storage and
  the ABI boundary work on byte offsets internally.
- **The bridge is explicit.** `stringToBytes :: String -> Array Int` and
  `bytesToString :: Array Int -> String` are the source-level conversions
  between the two representations. `stringToBytes` is lossless.
  `bytesToString` range-checks each element and then validates the whole
  sequence, so it traps on an out-of-range element or on malformed UTF-8. Their
  direction is Rust's: `String::from_utf8`, not a lossy decode.
- **Public APIs keep their meaning.** `WASI.Console.log :: String -> Effect Unit`
  is unchanged; the standard library converts at the call with `stringToBytes`.
  Only the bindings whose WIT type is genuinely `list<u8>` change signature, and
  they change to `Array Int`.

This supersedes the earlier requirement to preserve lone UTF-16 surrogates, and
it retires the UTF-16 storage and transcoder rather than leaving them as a
deviation.

## Consequences

- The language gains a real character repertoire: one `Char` is one visible
  character, emoji included, and no string can hold an unprintable value.
- The transcoder disappears. Strings cross the boundary as bytes, which removes
  a conversion, its buffer, and its replacement behavior.
- Malformed input is reported instead of repaired. A caller that wants
  replacement semantics must ask for them explicitly.
- `list<u8>` and `string` are distinguishable at the source level, so a WIT
  binding carries its own meaning. The cost is a copy plus a per-element range
  check on `list<u8>`, and a signature change in the standard library for every
  binding whose WIT type is `list<u8>`.
- Two upstream corpus cases move from "matches `purs`" to "differs from `purs`":
  the lone-surrogate escapes in `StringEscapes.purs`,
  `StringEdgeCases/Records.purs`, and `StringEdgeCases/Symbols.purs` are now
  rejected, and `failing/2434.purs` is now accepted. These are intentional
  differences, recorded with their measured numbers in
  [D-04](../design/D-04-suite-roadmap.md). The corpus files stay unchanged.
- The cost is one more representation boundary to reason about: a source string
  is scalar-indexed while its storage is byte-indexed. Every string operation
  has to state which of the two it works in.

Rejected alternatives:

- **Keep UTF-16 storage and the transcoder.** Preserves corpus agreement on
  lone surrogates, at the cost of a wider string array, a conversion at every
  boundary, and silent U+FFFD replacement. It also keeps a value in the language
  that has no valid rendering.
- **Store UTF-8 but decode leniently.** Rejects malformed sequences to U+FFFD
  like the transcoder did, so an invalid byte sequence still becomes a valid
  string and the error is never reported. Strict validation was chosen instead
  because it matches Rust, and because a repaired string is harder to debug than
  a trap.
- **Map `list<u8>` to `String` with an escape convention.** Avoids the copy and
  the signature churn, but invents an encoding the WIT type system does not
  have, and makes a byte-carrying import indistinguishable from a text one.
- **Use `externref` and let the host own string data.** The host would have to
  be trusted to produce canonical UTF-8, and the compiler could verify nothing.
- **Change the public WASI signatures to `Array Int`** rather than adding
  conversion intrinsics. That pushes an implementation detail into every user's
  call to `log`, and loses the distinction between a message and a byte buffer.
- **Expose the intrinsics only as compiler-internal boundary helpers.** Then a
  standard-library wrapper written in PureScript could not produce the byte
  buffer that `log` needs, so the library would have to be generated in another
  language.

## Implementation status

The migration has landed:

- The lexer decodes string and character literals to scalar sequences. A
  contiguous escaped surrogate pair is one scalar, an unpaired surrogate escape
  is rejected instead of becoming U+FFFD, and a supplementary scalar is one
  `Char`.
- Wasm GC strings are `(array (mut i8))` byte arrays holding canonical UTF-8. A
  distinct literal is a passive data segment materialized once with
  `array.new_data` and interned in a lazily initialized module global.
- The transcoder is replaced by strict UTF-8 validation and byte-preserving
  copies. A malformed sequence or an unpaired surrogate traps.
- A WIT `list<u8>` is `Array Int`, copied element by element with a `0..255`
  range check before narrowing, and never decoded as text.
- `stringToBytes` and `bytesToString` are the source-level bridge.
  `bytesToString` stages its input through a transient linear buffer, range-checks
  each element on the way in, and validates the sequence with the same boundary
  helper the ABI uses, so there is one implementation of the rule.
- Upstream corpus files are unchanged. The four cases above are recorded as
  intentional differences, and L1 parse agreement is 904/908 with exactly those
  four cases as the remainder.

Source-level scalar length, take, drop, slice and splitAt now execute through
Data.String.CodeUnits foreign-slot delegates to the independent PSRS.String
library. The library scans validated canonical UTF-8 and maps scalar indices
to byte boundaries before copying a range through PSRS.Array.sliceImpl. The
448-case pinned-FFI projection oracle makes the intentional scalar/UTF-16
difference explicit; see the [acceptance checkpoint](../implementation/stdlib/string-slicing-2026-10-07/report.md).
Scalar charAt and toChar now also execute through typed foreign-slot delegates.
The private library decoder reconstructs one scalar from validated canonical
UTF-8 before using the existing Int-to-Char identity primitive. The original
rank-N constructors and public pure wrappers are preserved. Their 269 projected
FFI observations and 6 builder checks are recorded in the
[character checkpoint](../implementation/stdlib/string-characters-2026-10-07/report.md).
Other foreign slots, including unsafe character indexing and predicate
traversal, remain unsupported. This does not establish the full String API.

This record is the semantic authority. The design documents state the contract,
including
[data representation](../design/backend/fp/data-representation.md),
[the linear-memory and canonical ABI boundary](../design/backend/wasm/linear-memory-and-canonical-abi-boundary.md),
[canonical ABI and WIT](../design/backend/wasm/canonical-abi-and-wit.md),
[lexing and layout](../design/frontend/syntax/lexing-and-layout.md),
[rows and records](../design/frontend/type-system/rows-and-records.md),
[classes and evidence](../design/frontend/type-system/classes-and-evidence.md),
[module resolution](../design/frontend/semantics/modules-and-resolution.md),
and [frontend and IR boundaries](../design/D-01-frontend-and-ir-boundaries.md).
