#!/bin/sh
set -eu
cd "$(dirname "$0")/../../.."
# Apply to dependencies as well: libm force_eval uses stack temporaries.
# Explicit frames let the linker measure their storage without a red zone.
RUSTFLAGS="${RUSTFLAGS:-} -C no-redzone=yes" \
cargo rustc --locked -p psrs-runtime --lib --no-default-features --features formatter \
  --target wasm32-unknown-unknown --profile target-runtime -- \
  -C link-arg=--import-memory -C link-arg=--global-base=65536 \
  -C link-arg=-zstack-size=65536 -C link-arg=--export=__heap_base
cargo run --locked -p psrs-runtime --example package -- \
  target/wasm32-unknown-unknown/target-runtime/psrs_runtime.wasm \
  "${1:-crates/psrs-runtime/artifact/psrs_runtime.wasm}"
