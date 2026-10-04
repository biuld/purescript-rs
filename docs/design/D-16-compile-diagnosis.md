# D-16 — Compile Diagnosis

**Status:** Stable (design)

**Implements:** [F-04 — Compile Diagnosis](../feature/F-04-compile-diagnosis.md)

## Purpose and scope

`psrs diagnose` records what the compiler actually did for each selected case.
It groups compile outcomes by their first reported blocker, records pass and
artifact boundaries, and compares compatible runs. A trace may retain selected
IR dumps and node lineage for focused investigations. Compile acceptance,
compile lineage, and Wasmtime runtime evidence are separate records with
separate claims.

The tool does not infer root causes from matching diagnostics, reconstruct
lineage from text dumps, alter compiler semantics, or treat compilation as
runtime validation. Official suite scoreboards remain the acceptance evidence
for roadmap gates.

## Commands

```sh
cargo run -p psrs-cli -- diagnose path/to/Main.purs --out /tmp/main.json
cargo run -p psrs-cli -- diagnose path/to/Main.purs --trace --out /tmp/main.json
cargo run -p psrs-cli -- diagnose path/to/Main.purs \
  --input path/to/Imported.purs --trace --out /tmp/explicit-inputs.json
cargo run -p psrs-cli -- diagnose --corpus passing --filter Functor --limit 20 \
  --out /tmp/functor.json --timeout 20
cargo run -p psrs-cli -- diagnose --compare /tmp/before.json /tmp/after.json
```

Without `--trace`, diagnosis records the run manifest, pass executions,
artifact identities, explicit input/output edges, diagnostics, and compact
canonical summaries only for representations that implement a versioned
summary. It does not clone or format complete IR values solely to make dumps.
With `--trace`, the compiler retains requested IR dumps on both accepted and
rejected cases and may attach node-level lineage when the producing pass
supports it. A replay bundle records the selected trace mode and forwards it
when replaying. Repeatable `--input FILE` adds explicitly ordered sources after
the primary file; this mode compiles exactly that list without rediscovering
imports. It is available only for file diagnosis, cannot be combined with
`--corpus`, and rejects duplicate canonical paths. The replay script first
runs a new diagnosis with the saved entry-first input list, trace mode, and a
separate `replay-trace.json`, then invokes the recorded `build` command on the
same files. These are separate compile attempts; the build command preserves
the original compile-failure exit status. Unsupported summary and lineage coverage is represented as
`unavailable` or `untracked`, never approximated with a Debug-text hash or
source-span overlap.

The corpus command uses `tests/upstream` unless `PURESCRIPT_REPO` selects an
official checkout. It applies the path filter before the limit. Cases with
foreign imports or adjacent JavaScript FFI files are recorded as `excluded`;
they do not count as passes. Each selected case runs in its own child process.
`--timeout` is a per-case deadline in seconds. A timeout or crash is captured
as a case result, and the batch continues. Progress and elapsed time are
printed to stderr. Compiler rejections are snapshot data and do not make the
diagnosis command itself fail; invalid options, missing files, or report I/O
errors do.

Querying a saved trace by case, symbol, or source span is part of the design.
Symbol queries resolve to declaration identities; span queries return every
linked node and its relation. If the saved coverage cannot answer a query, the
result says so. Re-capturing missing evidence creates a new linked run rather
than mutating an existing snapshot. The query command is not implemented by the
current first slice.

## Run and trace schema

The serialized format is versioned independently of compiler and pass
versions. A run manifest records:

- the selected case cohort and whether each case's complete input set was
  observed;
- ordered logical source identities and content fingerprints, including
  trusted-library inputs and their fingerprint, plus logical-to-physical path
  mappings when replay needs them;
- compiler revision, working-tree and executable fingerprints when available;
- target capabilities, optimization settings, relevant tool versions, and
  environment inputs when the runner can observe them;
- the trace mode, schema version, per-pass contract versions, summary versions,
  and explicit coverage gaps.

Unavailable environment facts stay absent or explicitly unavailable. A run
must not claim compatibility based on guessed defaults.

An artifact ID is unique within one run and identifies one produced snapshot;
it is not a content digest and equal content does not merge identities. Each
artifact record names its representation and format version, producer, storage
reference if retained, summary and digest state, and whether the artifact is
complete, partial, invalid, or unavailable. Production and validation are
different facts: validation records refer to artifact IDs and name the
validator, result, and observed coverage. A failed or skipped validator never
turns an artifact into a verified artifact.

A pass execution records a stable machine key, pass contract version, ordered
input and output artifact IDs, status (`completed`, `rejected`, `crashed`,
`timed_out`, or `not_run`), diagnostics, and observed validation references.
One execution may consume and produce multiple artifacts. Explicit edges tie
each artifact to the pass execution and its input or output role. In-place
optimization still creates a new artifact identity. Configuration, target
capabilities, effect context, and external bindings that affect a pass are
recorded as metadata inputs where available; the manifest marks any
unobserved dependency.

Diagnostics retain source, trusted-library, or program origin. When the
compiler can identify a trusted-library module, the diagnostic also records
that logical source identity and span; the broad `trusted_library` class alone
does not identify the file.

The complete backend pass sequence is represented at actual call boundaries:

```text
linked Core
  -> Core optimization
  -> Effect lowering and binding conformance
  -> closure conversion and CC verification
  -> MIR lowering and verification
  -> MIR optimization
  -> Wasm structuring and encoding
  -> component assembly and validation
  -> WAT printing
```

Nested verifier calls may initially have coarser coverage than their enclosing
pass. The trace states that granularity instead of synthesizing a separate
verifier event from a final success flag. Driver diagnostics and artifact
records use backend-owned pass/artifact events; serialization belongs to the
CLI layer, and lower-level IR crates do not depend on the CLI or its JSON
format.

## Source identity and lineage

Lineage is a many-to-many derivation relation. It is produced by the pass that
creates or transforms nodes; query code may index it but may not infer it from
formatted output.

- `SourceRef` identifies a logical source file by stable logical name and
  content fingerprint, with a byte span and an origin class (`user`,
  `trusted_library`, or `generated`). A run may separately map this identity to
  a physical path used for replay. A span locates source text; it is not a
  semantic identity.
- `DeclarationRef` identifies a module declaration by namespace and a stable
  cross-run key. Compiler-local numeric IDs are retained for within-run joins
  but are not treated as stable across runs.
- `NodeRef` is the pair of an artifact ID and a node ID local to that artifact.
- A derivation record names the producing execution, zero or more input nodes,
  zero or more output nodes, a relation role, and source/declaration origins.
  It has at least one input or output; elimination has one or more inputs and
  no outputs.

This shape expresses one-to-many expansion, many-to-one combination, and
elimination. For example, lowering one lambda may produce a lifted function,
an environment layout, and a closure construction; inlining may combine
several input nodes into several output nodes. Generated adapters record their
actual generation role and the input value, instantiation site, and owning
declaration when the pass has that evidence. Shared spans are query hints, not
proof of a derivation. Uninstrumented nodes are marked `untracked`.

## Comparison

`--compare` first checks compatibility of the selected case set, complete
source/import closure and load order, trusted-library contents, target and
relevant tool configuration, schema, pass contracts, and summary versions.
Compiler revision and executable fingerprints may differ because revisions
are the subject of comparison; both are shown. Incomplete source capture,
changed environment, unsupported versions, or coverage gaps are reported and
limit the claims that can be made.

For each matched case, comparison preserves compile status, blocker stage and
category, diagnostic details, and input-change semantics. Where compatible
canonical summaries exist, artifacts and pass executions are aligned by their
dependency edges and roles. The report names the earliest observed boundary
whose output differs. Independent branches may produce several incomparable
earliest differences. A missing artifact, pass, summary, or lineage record is
reported as an observation gap; it is not treated as an unchanged artifact.
Cross-run node IDs are never compared directly.

The initial trace implementation does not yet produce canonical IR summaries,
so artifact-content and earliest-artifact-difference comparison must report
`unavailable`. It may still compare the existing case statuses, blockers,
diagnostics, cohort, and source fingerprints.

## Runtime evidence

Compile traces end at produced Wasm/component artifacts. Runtime validation is
a separate `RuntimeRun` that references the exact Wasm artifact digest and
records the Wasmtime version and configuration, exit or trap result, and
value-sensitive assertions. A successful compile is never a runtime pass.
Runtime events do not become compiler pass executions. If a runtime provides a
Wasm function/type/instruction location, it may be joined to the Wasm artifact;
otherwise the raw trap is retained with the location marked unavailable.

## Implementation coverage

The backend and driver provide a versioned run-local artifact/pass model,
actual top-level backend pass IO boundaries, and an opt-in diagnosis API that
avoids dump capture by default. The CLI writes schema-v2 manifests, captures
selected dumps for accepted and rejected cases with `--trace`, replays explicit
ordered inputs, and preserves v1 status/blocker/input comparison. Its pass
comparison reports only the first observed execution-sequence difference
after checking cohort and complete per-case input compatibility. It is not an
artifact-content or DAG comparison, and it does not identify a root cause.
Observed host and installed `rustc`/`cargo` versions are reported separately;
the compiler build toolchain cannot be established from those observations and
remains unavailable.

The frontend trace is one coarse lowering boundary, and backend trace
validation is recorded only at the granularity the backend observes. Canonical
artifact summaries, source/declaration/node lineage, query commands, earliest
artifact-DAG diffing, and Wasmtime runtime records remain planned. Diagnosis
does not fix backend defects; in particular, an observed closure-signature
trap remains runtime evidence until a focused runtime test and compiler
investigation establish the cause and repair.
