//! The standard library, embedded as source.
//!
//! The bootstrap compiler has no module loader or linker, so the library is
//! parsed and merged into the program module before resolution. Every value it
//! provides is ordinary library code over `foreign import` declarations that
//! bind to WIT. This is a bootstrap mechanism, not the long-term module
//! system; see `docs/design/D-07-wit-imports-and-std.md`.

/// The standard library source merged into every compiled module.
pub const SOURCE: &str = r#"
module Prelude where

foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Int
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" writeStdout :: Int -> String -> Unit
foreign import "wasi:clocks/monotonic-clock#now" now :: Int

log :: String -> Unit
log s = let a = writeStdout getStdout s in writeStdout getStdout "\n"

error :: String -> Unit
error s = let a = writeStdout getStderr s in writeStdout getStderr "\n"
"#;
