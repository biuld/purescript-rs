#!/bin/sh
# Rebuilds the runtime artifact and checks that it reproduces the committed
# bytes. A mismatch means the reviewed provenance digest must be updated
# explicitly; this check never rewrites the artifact.
set -eu
cd "$(dirname "$0")/../../.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# Apply to dependencies as well: libm force_eval uses stack temporaries.
# Explicit frames let the linker measure their storage without a red zone.
RUSTFLAGS="${RUSTFLAGS:-} -C no-redzone=yes" \
cargo rustc --locked -p psrs-runtime --lib --no-default-features --features formatter \
  --target wasm32-unknown-unknown --profile target-runtime -- \
  -C link-arg=--import-memory -C link-arg=--global-base=65536 \
  -C link-arg=-zstack-size=65536 -C link-arg=--export=__heap_base
cargo run --locked -p psrs-runtime --example package -- \
  target/wasm32-unknown-unknown/target-runtime/psrs_runtime.wasm \
  "$tmp/psrs_runtime.wasm"
if cmp -s "$tmp/psrs_runtime.wasm" crates/psrs-runtime/artifact/psrs_runtime.wasm; then
  echo "runtime artifact reproduces byte-for-byte"
else
  echo "runtime artifact differs from the committed bytes; update the reviewed provenance" >&2
  exit 1
fi
