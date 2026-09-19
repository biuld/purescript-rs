# D-04 — Suite-Driven Roadmap

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

Turn the official PureScript test suite into a multi-stage roadmap by hand
classifying its corpus into coverage milestones. [DEC-04](../decision/DEC-04-official-test-suite-roadmap.md)
fixes the strategy: the suite is the compatibility target and the `purs`
compiler is the oracle. This document defines the milestones, the suite subset
each one is accountable for, and their acceptance criteria.

The milestones are ordered by dependency, not by suite size. Each milestone is
independently measurable as per-file agreement with `purs`, and each later
milestone reuses the earlier ones.

## Corpus

The vendored corpus is pinned to the PureScript `v0.15.16` release so that the
installed `purs` binary and the sources agree. Counts below are from that
revision and include files in subdirectories:

| Directory | Files | Role |
| --- | --- | --- |
| `tests/purs/layout` | 15 | Lexer/layout goldens with `.out` |
| `tests/purs/passing` | 439 | Must compile and run |
| `tests/purs/failing` | 444 | Must fail with a listed `errorCode` |
| `tests/purs/warning` | 68 | Must compile with listed warnings |
| `tests/purs/optimize` | 10 | Expected CoreFn output |

`purs --json-errors` reports a machine-readable `errorCode` per diagnostic.
Parsing occurs before module resolution, so `ErrorParsingModule` classifies
parse behavior even when the support libraries are not installed.

One caveat applies when using a single-file `purs compile` invocation without
the support libraries: for a module whose imports cannot be found, the compiler
reports `ModuleNotFound` from the partially-parsed header without parsing the
rest of the body. The harness therefore also supports classifying `failing`
files by their `@shouldFailWith` annotation (`PSRS_ORACLE=annotations`), which
is the corpus's own ground truth and does not depend on the installed
libraries or compiler revision.

## Exclusions

JavaScript and Node.js FFI are not a compatibility target, per [F-02](../feature/F-02-portable-programs.md).
A suite file is excluded from milestone acceptance when any of the following
holds:

- it contains a `foreign import` declaration;
- it is accompanied by a `.js` FFI implementation used by the same module;
- its expected `errorCode` is FFI-specific (`DeprecatedFFIPrime`,
  `MissingFFIImplementations`, `UnsupportedFFICommonJSImports`,
  `UnsupportedFFICommonJSExports`, `DeprecatedFFICommonJSModule`,
  `UnnecessaryFFIModule`, and similar).

In this corpus that is 26 `passing`, 32 `failing`, and 2 `warning` files, plus
25 `.js` files. Excluded files are reported separately by the harness and count
as neither agreement nor gaps. The parser may accept `foreign` syntax
opportunistically, but no milestone requires it.

Programs that use the project's PureScript-facing WASI libraries remain
targets; those libraries are implemented by the runtime, not by JavaScript
FFI.

## Milestones

| ID | Milestone | Primary suite subset | Layer |
| --- | --- | --- | --- |
| M0 | Layout and lexing | `layout` (15) | L0 |
| M1 | Surface grammar | parse behavior of all non-excluded files | L1 |
| M2 | Modules, imports, exports, names | resolution `errorCode`s | L2 |
| M3 | Kinds and higher-kinded types | kind `errorCode`s | L3 |
| M4 | Core type checking | type `errorCode`s | L4 |
| M5 | Type classes and instances | class `errorCode`s | L5 |
| M6 | Data, newtypes, records, rows | features required by `passing` | L4–L6 |
| M7 | Runtime and standard library | `passing` (414) run | L6 |
| M8 | Warnings and optimization | `warning` (68), `optimize` (10) | — |

### M0 — Layout and lexing

- **Suite:** `tests/purs/layout/*.purs` (15), for example `Shebang.purs`,
  `Delimiter.purs`, `Commas.purs`, `IntType.purs`, `AdoIn.purs`.
- **Goal:** Tokenization and the layout algorithm match PureScript's, including
  Unicode keywords (`∷`, `∀`, `→`), numeric and string literals, backtick
  operators, and `where`/`let`/`do`/`ado` layout.
- **Acceptance:** Our layout output is stable and reviewed against each golden;
  every layout file parses.
- **Prerequisite:** None.

### M1 — Surface grammar

- **Suite:** All non-excluded `passing` + `failing` + `warning` files.
- **Selection rule:** Files the oracle reports as `ErrorParsingModule` (34
  failing tests, for example `UnderscoreModuleName.purs`) must fail to parse;
  every other non-excluded file must parse successfully.
- **Goal:** The full surface grammar: module/import/export lists; value, type,
  `data`, `newtype`, `class`, `instance`, `derive`, `foreign`, fixity, and kind
  signature declarations; `where` blocks; types with `forall`, rows,
  constraints, kind annotations, application, and operators; expressions with
  `case`/`of` and guards, `do`/`ado`, records, sections, and type application;
  and pattern binders.
- **Acceptance:** L1 parse agreement reaches 100% over all four directories,
  enforced by the suite scoreboard.
- **Prerequisite:** M0.

**Progress (measured against the vendored `v0.15.16` corpus):** 906/908 parse
agreement (99.8%): `passing` 413/413, `failing` 413/413, `warning` 67/67, and
`layout` 13/15. The two remaining `layout` files exercise `case`/guard/backtick
layout combinations and are the only open M1 items. The numbers above use
`PSRS_ORACLE=annotations`; the default `purs` oracle additionally disagrees on
files whose imports prevent the installed compiler from parsing the body (see
the Corpus caveat).

### M2 — Modules, imports, exports, and names

- **Suite:** Failing tests by `errorCode`: `UnknownName` (22), `DeclConflict`
  (11), `TransitiveExportError` (10), `ExportConflict` (7), `ScopeConflict`
  (6), `TransitiveDctorExportError` (2), `OrphanTypeDeclaration` (2),
  `OrphanKindDeclaration` (2), `OverlappingNamesInLet` (4),
  `OverlappingArgNames` (2), `DuplicateValueDeclaration` (2), `DuplicateModule`
  (1), `CycleInModules` (1), `UnknownImport` (1),
  `UnknownImportDataConstructor` (1), `UnknownExport` (1),
  `UnknownExportDataConstructor` (1), `ModuleNotFound` (1).
- **Goal:** A module loader and namespace resolution: values, types,
  constructors, classes, imports with `hiding`/`as`, export lists, and
  dependency cycles.
- **Acceptance:** Agreement on the `errorCode`s above, and every `passing`
  module resolves.
- **Prerequisite:** M1.

**Progress (implemented slice):** the front end resolves a value-namespace
module graph: stable module IDs, duplicate (`DuplicateModule`), missing
(`ModuleNotFound`), and cyclic (`CycleInModules`) module diagnostics,
unqualified, qualified, and aliased imports, explicit and `hiding` import
lists (`UnknownImport`), explicit export lists (`UnknownExport`), and
cross-module value references. It also lowers `data`, `newtype`, `type`, and
`class` declarations into AST and HIR, resolves user type names and type-level
application in types, treats data constructors and class members as values, and
reports conflicts in the shared uppercase namespace (`DeclConflict`). Types,
constructors, and classes import and export across modules: `import M (T(..))`
brings the type and its constructors, `T(A, B)` selects constructors
(`UnknownImportDataConstructor`), and export lists validate constructors
(`UnknownExportDataConstructor`, `TransitiveDctorExportError`) and require
referenced types, superclasses, and class members to be exported too
(`TransitiveExportError`). An import with an `as` alias is qualified-only, so
`module A` re-exports validate through the alias: ambiguous aliases and
unaliased names report `ScopeConflict`, while the same name re-exported from
two modules reports `ExportConflict`. Surface lowering reports
`OrphanTypeDeclaration`, `OrphanKindDeclaration`, and `OverlappingArgNames`;
resolution reports `DuplicateValueDeclaration` and `OverlappingNamesInLet`.
Operator and fixity aliases, kind-annotation references, and value-type-based
transitive exports are still open.

**Measured baseline (annotations oracle):** the scoreboard also loads a case's
support modules from its sibling directory, matching the corpus layout. M2
failing agreement is 44/70 and 32/413 `passing` modules resolve, including all
11 `DeclConflict` cases, `ExportConflict` 5/7, `ScopeConflict` 5/6,
`UnknownImport`, `UnknownImportDataConstructor`, `UnknownExportDataConstructor`,
`TransitiveDctorExportError`, and the self-contained `TransitiveExportError`
subsets. The remaining failures need expression forms (records, `do`, guards,
`where`), instances, pattern binders, or fixity aliases rather than names.

### M3 — Kinds and higher-kinded types

- **Suite:** `KindsDoNotUnify` (29), `PartiallyAppliedSynonym` (12),
  `CycleInTypeSynonym` (4), `CycleInKindDeclaration` (4), `UndefinedTypeVariable`
  (4), `InfiniteKind` (2), `OrphanKindDeclaration` is M2 for resolution but kind
  errors are here, `UnsupportedTypeInKind` (1), `ScopedKindVariable` errors.
- **Goal:** Kinds, type synonyms, type constructors, type-level application,
  and kind checking.
- **Acceptance:** Agreement on the `errorCode`s above.
- **Prerequisite:** M2.

**Progress (implemented slice):** kinds are checked as a dedicated pass
(`psrs-kind`, P5) over resolved HIR. HIR and AST now carry kinded `forall`
binders, standalone kind signatures, kind annotations on type parameters,
records/rows, constraints, and type-level literals. The checker infers kinds for
`data`, `newtype`, `type`, and `class` declarations, unifies them with an occurs
check, and reports `KindsDoNotUnify`, `InfiniteKind`, `PartiallyAppliedSynonym`,
`CycleInTypeSynonym`, `CycleInKindDeclaration`, and `UndefinedTypeVariable`.
The driver exposes a lenient kind check and the `l3` scoreboard; the scoreboard
also runs against the vendored corpus without `purs`.

**Measured baseline (annotations oracle):** M3 failing agreement is 28/48.
Per code: `CycleInKindDeclaration` 2/2, `InfiniteKind` 2/2,
`CycleInTypeSynonym` 3/4, `UndefinedTypeVariable` 3/4,
`PartiallyAppliedSynonym` 8/12, `KindsDoNotUnify` 10/24. The remaining cases
need features outside the kind core: instances and `derive`, `foreign import
data`, rows in kinds, polymorphic expression annotations, and shared
cross-module kind environments.

### M4 — Core type checking

- **Suite:** `TypesDoNotUnify` (47), `HoleInferredType` (10), `EscapedSkolem`
  (3), `InfiniteType` (2), `ExpectedType` (2),
  `CannotApplyExpressionOfTypeOnType` (2), `AmbiguousTypeVariables` (1),
  `IntOutOfRange` (1), and related.
- **Goal:** The type checker over ADTs, records, rows, and functions, including
  holes and rigid variables.
- **Acceptance:** Agreement on the `errorCode`s above.
- **Prerequisite:** M3 and M6.

### M5 — Type classes and instances

- **Suite:** `NoInstanceFound` (55), `OverlappingInstances` (9),
  `OrphanInstance` (7), `InvalidInstanceHead` (7), `InvalidNewtypeInstance` (6),
  `ClassInstanceArityMismatch` (4), `MissingClassMember` (2),
  `PossiblyInfiniteInstance` (1), `DuplicateTypeClass` (1),
  `DuplicateInstance` (1), `CycleInTypeClassDeclaration` (2), `CannotDerive`
  (1), and related.
- **Goal:** Class and instance declarations, instance resolution, dictionary
  evidence, and `derive`.
- **Acceptance:** Agreement on the `errorCode`s above.
- **Prerequisite:** M4.

### M6 — Data, newtypes, records, and rows

- **Suite:** Feature subset of `passing` that uses `data` (324 files),
  `newtype` (80), type synonyms (143), records, and rows.
- **Goal:** Algebraic data types, constructors and pattern matching, newtype
  erasure, record and row types, and their runtime layouts.
- **Acceptance:** These `passing` files type-check and lower, and their
  `optimize` counterparts later match.
- **Prerequisite:** M1; supports M4, M5, and M7.

**Progress (implemented slice):** `data` and `newtype` constructors are
registered as polymorphic values and type-check, including application of
constructors with fields. A single-scrutinee `case` with constructor, variable,
and wildcard patterns type-checks, and constructor patterns in function
parameters lower to a temporary parameter plus `case`. The constructor table
flows through THIR and Core. The first runtime slice lowers the nullary
constructors of a non-parameterized data type to immediate integer tags and
`case` over it to tag comparisons, so enum-style programs compile to Wasm and
run under WASI. A valid single-field `newtype` is now erased in CC: construction
and matching pass through the field value, with no GC allocation. A first
concrete parameterized ADT slice also uses the selected erased representation:
fields that depend on a type parameter are boxed and recovered through `eqref`,
as demonstrated by `Maybe Int`. Concrete scalar array literals now also lower
to Wasm GC arrays. Fully polymorphic declarations, records, rows, array
operations, and richer heap or tagged aggregate layouts are still open, and
the backend reports them as named limitations. The parameterized ADT representation is
fixed by
[DEC-07](../decision/DEC-07-runtime-representation-for-parameterized-adts.md).
Supported non-parameterized fields use Wasm GC objects under the runtime baseline fixed by
[DEC-05](../decision/DEC-05-wasmtime-feature-set.md).

### M7 — Runtime and standard library

- **Suite:** `tests/purs/passing`, 414 files after excluding 26 FFI tests; these
  must compile and run with the expected observable result.
- **Goal:** Module loading, the PureScript-facing standard library (Prelude,
  `Effect`, console, assertions), closure conversion, and a runtime that
  executes the artifacts.
- **Acceptance:** Each passing test compiles and runs; failures are reported per
  file.
- **Prerequisite:** M2–M6.

### M8 — Warnings and optimization

- **Suite:** `warning` (68) with warning codes such as `UnusedName` (13),
  `WildcardInferredType` (8), `UserDefinedWarning` (8),
  `MissingTypeDeclaration` (8), `DuplicateExportRef` (7); and `optimize` (10)
  with expected CoreFn output.
- **Goal:** Warning diagnostics that match warning codes, and optimization
  output that matches the expected CoreFn shape.
- **Acceptance:** Warning-code agreement and optimize-output agreement.
- **Prerequisite:** M1 for warnings; M4 and M6 for optimize.

## Dependency graph

```text
M0 -> M1 -> M2 -> M3 -> M4 -> M5 -> M7
                        ^      ^
                        |      |
                       M6 -----+
M1 -> M8 (warnings)
M4, M6 -> M8 (optimize)
```

## Progress measurement

The suite harness classifies every file with `purs --json-errors` and reports
per-file agreement for the current milestone:

- `parse_source` measures M0–M1.
- `check_source` measures M2–M5.
- Runtime comparison measures M7.
- Warning and optimize comparison measure M8.

The harness is opt-in and skips without `purs` or a checkout, so
`cargo test --workspace` stays self-contained.

```sh
PURESCRIPT_REPO=/path/to/purescript \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

The resolution scoreboard reads the corpus's own `@shouldFailWith` annotations
and does not need `purs` or the support libraries, so it also runs against the
vendored corpus without `PURESCRIPT_REPO`.

Each milestone is complete only when its subset reaches 100% agreement. New
diagnostics must align to an official `errorCode`; message text and `.out`
formatting are not matched.

## Error-code appendix

Codes not listed above belong to the milestone whose layer matches their
semantics; the harness tracks unmapped codes so the mapping can grow. Notable
remaining groups:

- FFI and roles: `DeprecatedFFIPrime`, `MissingFFIImplementations`,
  `RoleMismatch`, `UnsupportedRoleDeclaration`, `InvalidCoercibleInstanceDeclaration`.
  FFI codes are excluded per the Exclusions section; role errors are deferred
  until the standard library requires them.
- Deriving: `CannotDeriveInvalidConstructorArg`, `CannotDeriveNewtypeForData`.
- Parse-adjacent: `MultipleValueOpFixities`, `MultipleTypeOpFixities`,
  `MixedAssociativityError`, `InvalidOperatorInBinder`.
- Module system: `CannotDefinePrimModules`, `DuplicateModule`, `CycleInModules`.

## Out of scope

- Matching diagnostic message text or `.out` byte content.
- IDE, docs, publish, sourcemaps, and graph test directories.
- Node.js and JavaScript FFI compatibility, per F-02.
