//! The MIR tag table: module-local exception identities for the Wasm
//! exception channel. See `docs/design/backend/fp/exception-handling-and-stack-exhaustion.md`.

use crate::types::ValueType;
use psrs_span::TextRange;

/// Module-local tag identity. Distinct from `FunctionId`, `DefinedTypeId`, and
/// every other index domain; a `TagId` indexes `Module::tags` by position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TagId(pub u32);

/// A compiler-defined tag: an exception kind with a payload type
/// `[t*] -> []`. The channel defines only module-local tags, so a tag is
/// neither imported nor exported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// At most one payload value; a multi-value payload needs the multi-value
    /// block work and is rejected by the verifier.
    pub payload: Vec<ValueType>,
    pub span: TextRange,
}
