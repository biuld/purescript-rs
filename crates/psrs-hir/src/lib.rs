use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};
use verify::verify_expr;

mod expr;
mod module;
mod substitution;
mod ty;
mod types;

pub use expr::{
    CaseBranch, Declaration, Expr, ExprKind, LocalBinder, LocalBinding, Pattern, PatternKind,
};
pub use module::{ExportList, ExportedSymbol, ExportedType, Import, ImportedSymbol, ImportedType};
pub use substitution::substitute_type_variables;
pub use ty::{BuiltinType, Type, TypeField, TypeKind, TypeParameter, TypeReference};
pub use types::{
    ClassMember, Constructor, DerivationStrategy, FunctionalDependency, InstanceDeclaration,
    InstanceMember, Role, RoleDeclaration, TypeDeclaration, TypeDeclarationKind,
};
pub use verify::VerifyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModuleId(pub u32);

impl ModuleId {
    pub const INTRINSICS: Self = Self(u32::MAX);
    /// Virtual module identity for compiler-provided source modules which
    /// contribute names but have no runtime module body of their own.
    pub const COMPILER_PRELUDE: Self = Self(u32::MAX - 1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolId {
    pub module: ModuleId,
    pub index: u32,
}

impl SymbolId {
    pub const fn new(module: ModuleId, index: u32) -> Self {
        Self { module, index }
    }
}

/// Identifies a user-defined type constructor, data constructor's parent type,
/// type synonym, or class within a module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId {
    pub module: ModuleId,
    pub index: u32,
}

impl TypeId {
    pub const fn new(module: ModuleId, index: u32) -> Self {
        Self { module, index }
    }

    /// Stable identity of the compiler-owned `Prim.Coerce.Coercible` class.
    pub const COERCIBLE: Self = Self::new(ModuleId::INTRINSICS, 0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Intrinsic {
    BoolTrue,
    BoolFalse,
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32RemS,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LeS,
    I32GtS,
    I32GeS,
    ArrayLength,
    ArrayIndex,
    ArrayUpdate,
    IntNeg,
    IntComplement,
    NumberNeg,
    BooleanNot,
    IntToNumber,
    NumberToInt,
    BooleanToInt,
    IntToBoolean,
    CharToInt,
    IntToChar,
    IntDiv,
    IntMod,
    IntAnd,
    IntOr,
    IntXor,
    IntShl,
    IntShr,
    IntZshr,
    NumberAdd,
    NumberSub,
    NumberMul,
    NumberDiv,
    NumberEq,
    NumberNe,
    NumberLt,
    NumberLe,
    NumberGt,
    NumberGe,
    BooleanAnd,
    BooleanOr,
    BooleanEq,
    BooleanNe,
    CharEq,
    CharNe,
    CharLt,
    CharLe,
    CharGt,
    CharGe,
    /// A source `String`'s canonical UTF-8 bytes as an `Array Int`. A source
    /// string is a sequence of Unicode scalar values, so this is lossless and
    /// never fails ([DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
    StringToBytes,
    /// An `Array Int` as a source `String`. Each element must be a canonical
    /// byte and the bytes must be well-formed UTF-8; either violation traps
    /// rather than producing replacement text.
    BytesToString,
    /// Source-level `Safe.Coerce.coerce`, elaborated to a checked coercion.
    Coerce,
}

impl Intrinsic {
    pub const fn symbol(self) -> SymbolId {
        SymbolId::new(ModuleId::INTRINSICS, self as u32)
    }
}

/// Symbol index base for source-declared `foreign import`s, which live in the
/// reserved intrinsic module but above the intrinsic and WASI import ranges.
pub const FOREIGN_SYMBOL_BASE: u32 = 1 << 24;

/// The kind of a known external value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalKind {
    /// A compiler primitive with a fixed lowering.
    Intrinsic(Intrinsic),
    /// A value imported from a WIT interface, declared in source with
    /// `foreign import "<interface>#<function>" name :: Type`. The backend
    /// resolves the canonical signature from the vendored WIT and lowers calls
    /// generically. See `docs/design/backend/wasm/canonical-abi-and-wit.md`.
    Wit { interface: String, function: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalSymbol {
    pub symbol: SymbolId,
    pub name: String,
    pub kind: ExternalKind,
    /// The declared type of a source-declared external (a `foreign import`).
    /// Compiler primitives and intrinsics have no declaration type here.
    pub signature: Option<Type>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

/// Identifies a generalized type variable. IDs are unique across a module so a
/// flat type table can keep them distinct without per-scheme scoping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVariableId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub imports: Vec<Import>,
    pub exports: Option<ExportList>,
    pub declarations: Vec<Declaration>,
    pub types: Vec<TypeDeclaration>,
    pub instances: Vec<InstanceDeclaration>,
    pub span: TextRange,
}

impl Module {
    /// Checks that every reference targets a declaration visible in this module.
    pub fn verify(&self) -> Result<(), Vec<VerifyError>> {
        let mut errors = Vec::new();
        let mut globals = HashSet::new();
        if self.id == ModuleId::INTRINSICS {
            errors.push(VerifyError {
                span: self.span,
                message: "module uses the reserved intrinsic module ID",
            });
        }
        for declaration in &self.declarations {
            if declaration.symbol.module != self.id {
                errors.push(VerifyError {
                    span: declaration.name_span,
                    message: "declaration symbol belongs to a different module",
                });
            }
            if !globals.insert(declaration.symbol) {
                errors.push(VerifyError {
                    span: declaration.name_span,
                    message: "duplicate declaration symbol ID",
                });
            }
        }
        for external in &self.externals {
            if external.symbol.module != ModuleId::INTRINSICS {
                errors.push(VerifyError {
                    span: self.span,
                    message: "external symbol does not use the intrinsic module ID",
                });
            }
            if !globals.insert(external.symbol) {
                errors.push(VerifyError {
                    span: self.span,
                    message: "duplicate global symbol ID",
                });
            }
        }

        for import in &self.imports {
            if import.module == ModuleId::INTRINSICS {
                errors.push(VerifyError {
                    span: import.span,
                    message: "import resolves to the reserved intrinsic module ID",
                });
            }
            for symbol in &import.symbols {
                if symbol.symbol.module == self.id {
                    errors.push(VerifyError {
                        span: symbol.span,
                        message: "imported symbol is declared in this module",
                    });
                }
                globals.insert(symbol.symbol);
            }
            for imported in &import.types {
                if matches!(imported.reference, TypeReference::Named(id) if id.module == self.id) {
                    errors.push(VerifyError {
                        span: imported.span,
                        message: "imported type is declared in this module",
                    });
                }
            }
        }

        let mut type_ids = HashSet::new();
        for declaration in &self.types {
            if declaration.id.module != self.id {
                errors.push(VerifyError {
                    span: declaration.name_span,
                    message: "type declaration belongs to a different module",
                });
            }
            if !type_ids.insert(declaration.id) {
                errors.push(VerifyError {
                    span: declaration.name_span,
                    message: "duplicate type declaration ID",
                });
            }
            for constructor in &declaration.constructors {
                if constructor.symbol.module != self.id {
                    errors.push(VerifyError {
                        span: constructor.name_span,
                        message: "constructor symbol belongs to a different module",
                    });
                }
                if !globals.insert(constructor.symbol) {
                    errors.push(VerifyError {
                        span: constructor.name_span,
                        message: "duplicate constructor symbol ID",
                    });
                }
            }
            for member in &declaration.members {
                if member.symbol.module != self.id {
                    errors.push(VerifyError {
                        span: member.name_span,
                        message: "class member symbol belongs to a different module",
                    });
                }
                if !globals.insert(member.symbol) {
                    errors.push(VerifyError {
                        span: member.name_span,
                        message: "duplicate class member symbol ID",
                    });
                }
            }
        }

        let mut imported_type_references = HashSet::new();
        for import in &self.imports {
            for imported in &import.types {
                imported_type_references.insert(imported.reference);
            }
        }
        if let Some(exports) = &self.exports {
            for exported in &exports.values {
                if !globals.contains(&exported.symbol) {
                    errors.push(VerifyError {
                        span: exported.span,
                        message: "exported symbol is not declared or imported",
                    });
                }
            }
            for exported in &exports.types {
                let declared = match exported.reference {
                    TypeReference::Builtin(_) => true,
                    TypeReference::Named(id) => {
                        type_ids.contains(&id)
                            || imported_type_references.contains(&exported.reference)
                    }
                };
                if !declared {
                    errors.push(VerifyError {
                        span: exported.name_span,
                        message: "exported type is not declared or imported",
                    });
                }
                if let Some(constructors) = &exported.constructors {
                    for constructor in constructors {
                        if !globals.contains(constructor) {
                            errors.push(VerifyError {
                                span: exported.name_span,
                                message: "exported constructor is not declared or imported",
                            });
                        }
                    }
                }
            }
        }

        let mut instance_chains: HashMap<u32, (u32, TypeId)> = HashMap::new();
        let mut completed_instance_chains = HashSet::new();
        let mut previous_instance_chain = None;
        for instance in &self.instances {
            if instance.symbol.module != self.id {
                errors.push(VerifyError {
                    span: instance.name_span,
                    message: "instance symbol belongs to a different module",
                });
            }
            if !globals.insert(instance.symbol) {
                errors.push(VerifyError {
                    span: instance.name_span,
                    message: "duplicate instance symbol ID",
                });
            }
            if let Some(previous) = previous_instance_chain
                && previous != instance.chain_id
            {
                completed_instance_chains.insert(previous);
            }
            if completed_instance_chains.contains(&instance.chain_id) {
                errors.push(VerifyError {
                    span: instance.name_span,
                    message: "instance-chain branches must remain contiguous",
                });
            }
            previous_instance_chain = Some(instance.chain_id);
            match instance_chains.get_mut(&instance.chain_id) {
                Some((next_position, class_id)) => {
                    if *class_id != instance.class_id {
                        errors.push(VerifyError {
                            span: instance.name_span,
                            message: "one instance chain contains different classes",
                        });
                    }
                    if instance.chain_position != *next_position {
                        errors.push(VerifyError {
                            span: instance.name_span,
                            message: "instance chain positions must be contiguous and ordered",
                        });
                    }
                    *next_position = instance.chain_position.saturating_add(1);
                }
                None => {
                    if instance.chain_position != 0 {
                        errors.push(VerifyError {
                            span: instance.name_span,
                            message: "an instance chain must begin at position zero",
                        });
                    }
                    instance_chains.insert(
                        instance.chain_id,
                        (instance.chain_position.saturating_add(1), instance.class_id),
                    );
                }
            }
            if instance.class_id.module == self.id {
                if !type_ids.contains(&instance.class_id) {
                    errors.push(VerifyError {
                        span: instance.name_span,
                        message: "instance class is not declared in this module",
                    });
                }
            } else if !imported_type_references.contains(&TypeReference::Named(instance.class_id)) {
                errors.push(VerifyError {
                    span: instance.name_span,
                    message: "instance class is not declared or imported",
                });
            }
        }

        let mut declared_locals = HashSet::new();
        for declaration in &self.declarations {
            let mut visible_locals = HashSet::new();
            verify_expr(
                &declaration.value,
                &globals,
                &mut visible_locals,
                &mut declared_locals,
                &mut errors,
            );
        }
        for instance in &self.instances {
            for member in &instance.members {
                let mut visible_locals = HashSet::new();
                verify_expr(
                    &member.value,
                    &globals,
                    &mut visible_locals,
                    &mut declared_locals,
                    &mut errors,
                );
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests;
mod verify;
