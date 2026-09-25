//! The embedded PureScript standard library and WASI platform modules.
//!
//! The platform-independent `Prelude` defines the small `Effect` abstraction;
//! platform services live in explicit `WASI.*` modules and are linked like
//! ordinary source modules.

/// The name of the standard-library module.
pub const NAME: &str = "Prelude";

/// The platform-independent standard-library source.
pub const SOURCE: &str = r#"
module Prelude where

data Effect a

pure :: forall a. a -> Effect a
pure value = \token -> value

bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b
bind first next = \token -> next (first token) token

runEffect :: forall a. Effect a -> a
runEffect action = action 0
"#;

pub const CONSOLE_SOURCE: &str = r#"
module WASI.Console where

import Prelude

foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Int
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" writeStdout :: Int -> String -> Unit

log :: String -> Effect Unit
log s = \token -> let ignored = writeStdout getStdout s in writeStdout getStdout "\n"

error :: String -> Effect Unit
error s = \token -> let ignored = writeStdout getStderr s in writeStdout getStderr "\n"
"#;

pub const CLOCK_SOURCE: &str = r#"
module WASI.Clock where

import Prelude

foreign import "wasi:clocks/monotonic-clock#now" monotonicNow :: Int

now :: Effect Int
now = \token -> monotonicNow
"#;

/// Sources hidden from callers but included in every compile pipeline.
pub const SOURCES: &[(&str, &str)] = &[
    (NAME, SOURCE),
    ("WASI.Console", CONSOLE_SOURCE),
    ("WASI.Clock", CLOCK_SOURCE),
];
