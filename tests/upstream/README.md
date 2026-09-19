# Upstream Test Corpus

This directory vendors the PureScript compiler's test suite so the compatibility
roadmap in `docs/design/D-04-suite-roadmap.md` runs hermetically. Only `.purs`
sources are vendored; golden `.out` files and `.js` FFI implementations are
not, because diagnostics are aligned by `errorCode` (not message text) and
JavaScript FFI is out of scope.

- Source: the PureScript compiler repository, `tests/purs/{passing,failing,warning,layout}`.
- The vendored files retain their original license; see the upstream repository.
- Layout: the directory structure is preserved, including multi-file tests in
  subdirectories.

## Updating

Re-copy the four directories from a checkout, keeping `.purs` files only:

```sh
rsync -a --include='*/' --include='*.purs' --exclude='*' \
  "$PURESCRIPT_REPO/tests/purs/passing" \
  "$PURESCRIPT_REPO/tests/purs/failing" \
  "$PURESCRIPT_REPO/tests/purs/warning" \
  "$PURESCRIPT_REPO/tests/purs/layout" \
  tests/upstream/
```

## Exclusions

Files that declare `foreign import`, ship a `.js` FFI implementation, or expect
an FFI-specific `errorCode` are excluded from milestone acceptance. The harness
reports them as excluded, never as agreement or gaps.

## Running

```sh
cargo test -p psrs-driver --test upstream
PURESCRIPT_REPO=/path/to/purescript \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

`PURESCRIPT_REPO` overrides the vendored corpus with a live checkout's
`tests/purs` when set. The suite test requires `purs` and skips otherwise.
