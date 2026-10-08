#!/bin/sh
set -eu
cd "$(dirname "$0")/../../.."
sysroot=$(rustc --print sysroot)
# The upper 16 KiB of the stack prefix is allocator-owned; verified frame
# bounds keep accesses above canonical scratch. Data starts above the stack.
# Paths in panic text must not capture this machine.
RUSTFLAGS="${RUSTFLAGS:-} -C no-redzone=yes --remap-path-prefix=${HOME}/.cargo/=/cargo/ --remap-path-prefix=${sysroot}=/rustc/ --remap-path-prefix=${PWD}=/psrs/" \
cargo rustc --locked -p psrs-runtime --lib --no-default-features --features allocator \
  --target wasm32-unknown-unknown --profile target-runtime -- \
  -C link-arg=--import-memory -C link-arg=--global-base=32768 \
  -C link-arg=--stack-first -C link-arg=-zstack-size=32768
cp target/wasm32-unknown-unknown/target-runtime/psrs_runtime.wasm \
  "${1:-crates/psrs-runtime/artifact/psrs_allocator.wasm}"
