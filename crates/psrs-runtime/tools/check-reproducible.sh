#!/bin/sh
# Rebuild both runtime artifacts without overwriting the reviewed bytes.
set -eu
cd "$(dirname "$0")/../../.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
sh crates/psrs-runtime/tools/build.sh "$tmp/psrs_runtime.wasm"
sh crates/psrs-runtime/tools/build-allocator.sh "$tmp/psrs_allocator.wasm"
for artifact in psrs_runtime psrs_allocator; do
  if cmp -s "$tmp/$artifact.wasm" "crates/psrs-runtime/artifact/$artifact.wasm"; then
    echo "$artifact reproduces byte-for-byte"
  else
    echo "$artifact differs from the reviewed bytes; update its provenance" >&2
    exit 1
  fi
done
