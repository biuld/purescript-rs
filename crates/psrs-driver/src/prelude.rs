//! The standard library, embedded as source.
//!
//! The library is a real module: it is linked into the program with every other
//! module, and a program reaches its values through an ordinary import. The
//! compiler has no module loader yet, so the source is embedded and provided to
//! the pipeline. See `docs/design/D-07-wit-imports-and-std.md`.

/// The name of the standard-library module.
pub const NAME: &str = "Prelude";

/// The standard library source, linked into every compiled program.
pub const SOURCE: &str = r#"
module Prelude where

foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Int
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" writeStdout :: Int -> String -> Unit
foreign import "wasi:clocks/monotonic-clock#now" monotonicNow :: Int

now :: Int
now = monotonicNow

log :: String -> Unit
log s = let a = writeStdout getStdout s in writeStdout getStdout "\n"

error :: String -> Unit
error s = let a = writeStdout getStderr s in writeStdout getStderr "\n"
"#;
