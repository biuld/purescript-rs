//! GC storage ABI, separate from scalar/linear-memory artifact contracts.
//! State dependencies have no physical parameters or results in this ABI.

mod projection;
pub use projection::*;

pub const STORAGE_MODULE: &str = "psrs:runtime-storage";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageType {
    I32,
    /// A non-null mutable Wasm GC array of nullable eqref values.
    Array,
    /// A nullable eqref; checked callers own boxing and payload validation.
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageOperation {
    Fill,
    Read,
    Write,
    Trap,
}

/// Checked source-value relations before GC boxing and state projection.
/// Element occurrences in one binding must denote the same source type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageValue {
    Int,
    Unit,
    Element,
    Array,
    /// Only a non-returning operation may have an arbitrary payload type.
    Any,
}

pub struct StorageSourceContract {
    /// Ordinary parameters before the final logical State parameter.
    pub parameters: &'static [StorageValue],
    pub payload: StorageValue,
}

pub struct StorageAbi {
    pub export: &'static str,
    pub parameters: &'static [StorageType],
    pub result: Option<StorageType>,
    /// A trap operation has no normal return, despite its physical void type.
    pub returns_normally: bool,
}

impl StorageOperation {
    pub const ALL: [Self; 4] = [Self::Fill, Self::Read, Self::Write, Self::Trap];

    pub fn from_export(export: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.abi().export == export)
    }

    /// State regions are generic and must agree between input and successor.
    pub const fn source_contract(self) -> StorageSourceContract {
        use StorageValue::*;
        match self {
            Self::Fill => StorageSourceContract {
                parameters: &[Int, Element],
                payload: Array,
            },
            Self::Read => StorageSourceContract {
                parameters: &[Array, Int],
                payload: Element,
            },
            Self::Write => StorageSourceContract {
                parameters: &[Array, Int, Element],
                payload: Unit,
            },
            Self::Trap => StorageSourceContract {
                parameters: &[],
                payload: Any,
            },
        }
    }

    pub const fn abi(self) -> StorageAbi {
        use StorageType::*;
        match self {
            Self::Fill => StorageAbi {
                export: "array_fill",
                parameters: &[I32, Value],
                result: Some(Array),
                returns_normally: true,
            },
            Self::Read => StorageAbi {
                export: "array_read",
                parameters: &[Array, I32],
                result: Some(Value),
                returns_normally: true,
            },
            Self::Write => StorageAbi {
                export: "array_write",
                parameters: &[Array, I32, Value],
                result: None,
                returns_normally: true,
            },
            Self::Trap => StorageAbi {
                export: "trap",
                parameters: &[],
                result: None,
                returns_normally: false,
            },
        }
    }
}
