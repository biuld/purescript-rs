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

/// The runtime signature of a non-void function value or call target.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionSignature {
    pub parameters: Vec<ValueType>,
    pub result: ValueType,
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
    Index(DefinedTypeId),
}

/// A module-local ID for one concrete defined type owned by MIR. This is not
/// a final Wasm type index; P10 assigns those only when it builds the thin
/// Wasm module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefinedTypeId(pub u32);

/// Module-local resource IDs. They are intentionally separate from final Wasm
/// indices, which are assigned by P10 after all MIR resources are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub u32);

/// A module-local slot in the linear closure function table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableSlot(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemoryId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataId(pub u32);

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
    pub supertype: Option<DefinedTypeId>,
    pub composite: CompositeType,
}

/// A recursion group of defined types. Types in a group may refer to each
/// other; the group flattens into consecutive entries of the type index space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecGroup(pub Vec<DefinedType>);

/// A virtual value in a low-level IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);

/// A typed virtual value declaration. Shared by CC IR and MIR so the value
/// model is language-agnostic and owned below Typed Core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDecl {
    pub id: ValueId,
    pub ty: ValueType,
}
