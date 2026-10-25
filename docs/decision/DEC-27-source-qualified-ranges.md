# DEC-27 — Source-qualified ranges across linked representations

**Status:** Accepted\
**Date:** 2026-10-25\
**Design:** [D-01](../design/D-01-frontend-and-ir-boundaries.md)

## Context and constraints

A file-local byte range does not identify a location after several modules are
linked and their bodies are inlined. A measured two-module example diagnoses
Helper's bytes 153..165 against a 58-byte Main file. The current containing
function cannot recover the original file after copying its callee's body.
The same information is required by initialization-demand errors: the reference
reports the failing read's line, not the binding's declaration line.

Source provenance must survive linking, cloning, substitution, capture lifting
and backend lowering. CST remains syntax-only. No declaration identity, checked
type, runtime layout or resolver object belongs in a syntax range. Hand-built
and unlinked IR must remain usable without pretending to have real source text.

## Decision

`psrs-span::TextRange` carries an optional opaque, compilation-local `SourceId`
in addition to its byte coordinates. `new` and `empty` make file-relative
ranges. `in_source` qualifies only an unqualified range; it never rebases an
already qualified location when its expression moves. Equality and hashing
include the source identity, so equal offsets in different files are distinct.
The type is shared source metadata, not a resolved declaration or type ID.

Core linking attaches the incoming source unit's identity while doing its
existing type/variable conversion. In this pipeline source units and module
IDs correspond one-to-one, so the Core owner explicitly projects that identity
to `SourceId`. The traversal covers declaration and binder ranges, expressions,
let bindings, case branches and nested patterns. It preserves prior qualification
when linking already qualified bodies. It does not add a second expression
walk or a feature-specific location table.

Subsequent passes preserve the complete range when copying existing nodes.
Fresh administrative nodes use their real construction anchor; they must not
reconstruct a moved node's location from numeric offsets and a new enclosing
module. Core verification and backend diagnostics prefer the qualified range's
source, using an explicit context only when the range remains file-relative.
This applies to errors and warnings at their common construction boundaries.

A source identity does not itself provide a module name, file contents or line
number. Runtime location rendering still requires an authoritative source
catalogue, validated identity lookup and in-bounds coordinates. In particular,
`SourceFile::line_column` clamps offsets for rendering convenience and cannot
serve as a source-range validity check. Missing real metadata must not become
a fabricated location.

The driver supplies Core's `SourceCatalogue` from each actual source unit and
its resolved module name before linking, including the trusted prefix. Linking
combines the immutable, shared source units; cloning and optimization preserve
them. The catalogue retains duplicate identities so a lookup rejects ambiguity
rather than overwriting a file. Location lookup checks source identity, byte
ordering/bounds and both UTF-8 boundaries before mapping a line. Semantic Core
verification trusts producer-assigned provenance; rendering validates the
catalogue/range contract it actually needs. Missing real metadata is an explicit
compiler-IR error at that use, including low-level Core entry points.

P8's recursive getter-force contract carries a String message for each force.
The local access entry retains immutable binding-name/module context through
capture; the message uses the read's preserved range. The getter uses this
operand only on Initializing. It does not cache the exception or retry work.

## Consequences

Ranges become larger and source-aware equality can expose tests which previously
compared only coordinates. Callers construct them through the range API rather
than public two-field struct literals. Pure coordinate operations remain valid
on file-local syntax; source-aware transformations retain the qualified value.
IDs are local to one compilation, not persistent filesystem identities.
The plain CST case RHS stores its expression through a box so the larger range
does not inflate the enum's inline payload beyond the workspace size check.

Adding an origin only to Core expressions would leave binder, pattern and later
IR ranges without a common ownership rule. A side table keyed by byte offsets
would collide across files and require every cloning pass to reinterpret it.
Inferring the source from a current function is already disproved by the
retained regression. Shared range metadata keeps that fact in one owner while
leaving source naming and line mapping outside the IR's semantic content.

This decision does not implement source-error raising or claim that the
initialization trap is repaired. Required acceptance includes cross-module
inlined errors and warnings, range qualification/movement tests, existing Core
verification, and full workspace validation before landing the implementation.
