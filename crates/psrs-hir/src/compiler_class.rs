//! Canonical library interfaces whose dictionaries the compiler can construct.
//! Resolution attaches this identity once; aliases and re-exports preserve the
//! declaration's TypeId and consumers never dispatch on source name spelling.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompilerClass {
    IsSymbol,
}

impl CompilerClass {
    pub fn method(self) -> &'static str {
        match self {
            Self::IsSymbol => "reflectSymbol",
        }
    }
}
