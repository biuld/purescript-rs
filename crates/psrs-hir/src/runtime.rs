//! The bootstrap runtime ABI registry.
//!
//! Host functions the compiler exposes are declared as data—name, symbol, and
//! resolved type—rather than as an enum variant per function. Adding a host
//! function is an entry here plus a backend lowering, not a change to an enum.
//! See `docs/decision/DEC-03-purescript-faithful-type-system.md`.

use crate::{BuiltinType, ModuleId, SymbolId, Type, TypeKind};
use psrs_span::TextRange;

/// Host function symbols start above the intrinsic range so the two never
/// collide inside the reserved intrinsic module.
const SYMBOL_BASE: u32 = 1 << 16;

/// A host function the bootstrap exposes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFunction {
    pub name: &'static str,
    pub symbol: SymbolId,
    pub ty: Type,
}

impl HostFunction {
    /// The number of arguments the host function takes.
    pub fn arity(&self) -> u32 {
        let mut ty = &self.ty;
        let mut arity = 0;
        while let TypeKind::Function { result, .. } = &ty.kind {
            arity += 1;
            ty = result;
        }
        arity
    }

    /// Whether the result type is `Boolean`, the backend's non-integer scalar.
    pub fn returns_boolean(&self) -> bool {
        let mut ty = &self.ty;
        while let TypeKind::Function { result, .. } = &ty.kind {
            ty = result;
        }
        matches!(&ty.kind, TypeKind::Constructor(BuiltinType::Boolean))
    }
}

/// The host functions available to every bootstrap module.
pub fn host_functions() -> Vec<HostFunction> {
    vec![HostFunction {
        name: "log",
        symbol: SymbolId::new(ModuleId::INTRINSICS, SYMBOL_BASE),
        ty: string_to_unit(),
    }]
}

/// Looks up a host function by the symbol the resolver assigned.
pub fn host_function_by_symbol(symbol: SymbolId) -> Option<HostFunction> {
    host_functions()
        .into_iter()
        .find(|function| function.symbol == symbol)
}

/// Looks up a host function by its source name.
pub fn host_function(name: &str) -> Option<HostFunction> {
    host_functions()
        .into_iter()
        .find(|function| function.name == name)
}

fn string_to_unit() -> Type {
    let constructor = |builtin| Type {
        kind: TypeKind::Constructor(builtin),
        span: TextRange::new(0, 0),
    };
    Type {
        kind: TypeKind::Function {
            parameter: Box::new(constructor(BuiltinType::String)),
            result: Box::new(constructor(BuiltinType::Unit)),
        },
        span: TextRange::new(0, 0),
    }
}
