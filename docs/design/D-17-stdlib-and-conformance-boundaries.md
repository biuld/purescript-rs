# Standard-library package and conformance boundaries

Matching features: [F-02](../feature/F-02-portable-programs.md) and
[F-04](../feature/F-04-compile-diagnosis.md).

## Ownership

`psrs-stdlib` is a separate source package and local Git repository. It owns
official library sources, exact upstream package pins and SHA-256 source hashes,
target adaptations, and library behavior cases. Its initial history is the
compiler's `stdlib/` subtree history; the move preserves source bytes.

The compiler owns language semantics, checked foreign binding identity and type
evidence, supported binding protocols, lowering, and runtime representation.
The library owns array algorithms over small runtime/storage primitives;
checked primitive calls and allocation/write semantics belong to the compiler.
Whole stdlib functions are not automatically intrinsic candidates.
Library source changes cannot compensate for compiler defects. Follow the
[source-fidelity contract](../workflow/stdlib-vendoring.md).

Executable Core linking retains static library foreign declarations and their
checked signatures only when reached from the entry, just as it retains ordinary
library values. A reached unimplemented binding remains a linking error.
Explicit primitive and WIT declarations retain source-wide protocol validation,
including unused declarations. This executable reachability contract does not
establish library support: inventories and public API execution cases must expose
missing implementations independently of dead-code removal.

`psrs-stdlib/tools/conformance.mjs` is a library-owned Node component. It compares
source inventories, evaluates pinned upstream JavaScript implementations, and
runs a compiler executable and Wasmtime. Inputs are package paths, manifests,
case data, and executable paths. It uses Node built-ins, requires no npm
installation, and has no compiler-internal Rust dependency. Its implementation
and tests evolve with the library; the compiler retains package locking and
Rust integration tests.
A future cargo xtask may invoke these commands without becoming their owner.

## Package selection

`stdlib.lock.json` records a package revision, relative local checkout path,
and portable content fingerprint. The development default is `../psrs-stdlib`;
no remote publishing or download mechanism is implied. A fresh checkout or CI
worker must provision the separate package before library-dependent checks.
Automatic CI acquisition remains pending a published source location. The default loader
rejects a content mismatch. The revision records provenance; archives do not
require Git. Content identity, rather than an unchecked Git HEAD, governs load.

`PSRS_STDLIB_ROOT` explicitly selects an unlocked development package. An invalid
selection fails without falling back. Installed executables currently require
this variable when their build-time checkout and lock are unavailable.
The driver exposes the selected package metadata and caches the parsed sources
for the process lifetime. Changes during initial loading are rejected; restart
the process after editing a development package.

The manifest declares binding protocol version 1, Unicode scalar/UTF-8 strings,
and signed i32 integers. Unsupported or missing protocol fields are errors.
The existing import-closure selection and compiler-provided module ownership
remain in the driver and resolver respectively.

## Reproducible content identity

The `fnv1a64-v1:` identifier is a reproducibility fingerprint, not a cryptographic
integrity check. Starting at FNV-1a's 64-bit offset basis, hash the bytes
`psrs-stdlib-content-v1` followed by NUL. Sort relative paths lexicographically
by UTF-8 path components from `lib/`, `conformance/`, `manifest.json`, and `upstream-lock.json`. For each,
hash its UTF-8 path, NUL, file length as unsigned 64-bit little-endian bytes,
and the exact file bytes. FNV multiplication wraps at 64 bits. Package symlinks
and special files are rejected. The upstream lock retains cryptographic hashes.
Tooling and docs are outside this package content fingerprint; record the library tool revision with evidence.
Moving tools does not change the source/case fingerprint.

The CLI obtains the fingerprint from the driver's selected package, including
an override. This changes the diagnosis cohort identity from the previous
absolute-path fingerprint; old and new snapshots are not the same cohort.

## Evidence and remaining work

Audit reports must compare the complete package set to the package's pinned
upstreams. Scalar oracle generation reads case data from the library package,
checks upstream revisions and clean checkouts, retains raw official results,
and records explicit target representation differences.

Runtime reports identify compiler and Wasmtime binaries, input sources, package
content, generated Wasm, exact commands, timeouts, exit codes, and stdout/stderr
bytes. Compile failure, launch failure, timeout, and an observation mismatch
cannot pass. Missing Wasmtime is an error. Scalar observations cover only the
implemented bindings; they do not establish whole-library FFI correctness.

The package manifest remains `incomplete`. A separate repository does not imply
release readiness, full compile acceptance, or complete runtime support. Future
publishing must add acquisition, license packaging, release compatibility, and
broader value-sensitive conformance evidence before claiming those properties.
