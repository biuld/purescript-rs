# Explicit Component Linking

**Feature:** [F-02](../feature/F-02-portable-programs.md).

**Design:** [Linking and Runtime](../design/backend/wasm/linking-and-runtime.md).

Use an already built synchronous application component and explicitly pinned
whole-interface guest providers:

```sh
psrs link application.wasm --manifest providers.json -o linked.wasm --report link.json
```

The command composes components; it does not compile PureScript source, infer
providers from WIT files, download artifacts or discover providers by directory.
Source compilation can explicitly request the same checked composition:

```sh
psrs build Main.purs --manifest providers.json -o linked.wasm --report build.json
```

`build` preserves source/stdlib checking and completes its core runtime plan
before composing the resulting application component. It does not discover
providers. For compiler-produced roots, `application_sha256` is optional: the
compiler pins the exact bytes it just emitted. If supplied, the pin must match.
Standalone `link` always requires the root pin.

## Manifest version 1

```json
{
  "schema_version": 1,
  "application_sha256": "<64 lowercase hexadecimal characters>",
  "providers": [
    {
      "id": "service",
      "path": "components/service.wasm",
      "sha256": "<64 lowercase hexadecimal characters>"
    }
  ],
  "bindings": [
    {
      "interface": "example:service/api@1.0.0",
      "provider": "service"
    }
  ],
  "permitted_host_interfaces": ["wasi:cli/stdout@0.2.12"]
}
```

Provider paths resolve relative to the manifest. The application path resolves
relative to the command's working directory. Artifact identities must be unique;
`application` is reserved for the root. Every live artifact's SHA-256 must match
its exact executable bytes. Unknown fields and schema versions are errors.

A binding applies wherever its interface occurs in the live graph, including
transitive guest imports. An optional `export` field defaults to `interface`;
the initial format requires the two canonical names to match exactly. There is
one provider instance per selected artifact identity, shared by its consumers.
Bindings select whole interfaces, preserving the identity of resources and their
constructor, method and destructor operations. The component type checker rejects
incompatible function shapes and resource connections.

Unbound live imports must be explicitly permitted host interfaces. The command
also checks WASI permissions against the compiler's stable target world and
capability profile. Other host interfaces form an explicit custom host contract;
the runtime must supply them. Unused candidate artifacts do not enter the graph
or report. Declaring an interface as permitted does not rescue a missing or
incompatible explicitly selected guest.

Only instance imports and synchronous composition under the stable Wasm feature
profile are supported. Definition-only WIT packages, non-interface imports,
implicit version adaptation and cyclic instantiation cannot satisfy this entry
point. Guest executable core/component start functions are rejected: version 1
has no provider initialization-effect contract. Declarative memory/data/global
initialization stays within the typed component; the compiler-owned application
may contain the already checked encoder's shim initialization. Provider memory stays inside that component; canonical lift/lower,
realloc and post-return carry values across the connection.

## Report and evidence

The optional report records schema version, selected component identities and
SHA-256 pins (including the application), live provider binding edges, validator
feature bits, the exact residual host interface imports, and the output digest.
A successful report and binary are written only after planning and validation.
With `build --report`, a rejected guest plan writes a `status: rejected` report
with its source lineage, exact application digest and composition diagnostic;
it produces no linked binary or successful output digest. Standalone `link`
reports remain successful-plan records. A report establishes artifact composition and validation, not successful execution or
source-language conformance. Standalone `link` reports are separate from
`psrs diagnose`. A `build --report` report includes the same diagnosis source/pass/artifact vocabulary, with a
`component_output` join naming the produced trace artifact and its exact SHA-256.
`application_sha256` matches that join and the composition plan's root pin; the
composition output digest matches the final file. It also records the exact
manifest digest. These links establish source-to-component lineage without
claiming runtime observations as compile verification.

Run the resulting command component with a compatible runtime:

```sh
wasmtime run linked.wasm
```

The linker tests execute cross-component strings, lists/results, post-return
buffer invalidation and resource lifetime behavior. CLI tests cover manifest
relative paths, digest rejection, closed imports and report/output agreement.
