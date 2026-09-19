//! The language-agnostic WebAssembly 3.0 value and type model shared by the
//! low-level IRs. See `docs/design/D-06-low-level-ir-and-wasm-types.md`.

/// A WebAssembly value type. `Boolean` is a logical convenience that the
/// low-level IRs represent as `i32`; every other variant is a Wasm value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueType {
    I32,
    Boolean,
    I64,
    F32,
    F64,
    Ref(RefType),
}

/// A WebAssembly reference type: nullable or not, referring to a heap type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RefType {
    pub nullable: bool,
    pub heap: HeapType,
}

/// A WebAssembly heap type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeapType {
    Func,
    Extern,
    Any,
    Eq,
    I31,
    Struct,
    Array,
    /// A defined type in the module's type table.
    Index(u32),
}

/// The storage type of a struct or array field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StorageType {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    V128,
    Ref(RefType),
}

/// A field of a struct or array type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldType {
    pub storage: StorageType,
    pub mutable: bool,
}

/// A defined (composite) type: a function, struct, or array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositeType {
    Func {
        parameters: Vec<ValueType>,
        results: Vec<ValueType>,
    },
    Struct(Vec<FieldType>),
    Array(FieldType),
}

/// A defined type with its subtyping declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinedType {
    pub final_type: bool,
    pub supertype: Option<u32>,
    pub composite: CompositeType,
}

/// A recursion group of defined types. Types in a group may refer to each
/// other; the group flattens into consecutive entries of the type index space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecGroup(pub Vec<DefinedType>);
