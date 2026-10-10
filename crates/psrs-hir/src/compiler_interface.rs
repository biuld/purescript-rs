//! Canonical compiler/library interface bindings, owned at name resolution.
//! This registry declares identities, not typechecker or runtime behavior.
//! Source implementations keep their declarations and ordinary module TypeIds.
use super::{CompilerClass, Intrinsic, TypeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceImplementation {
    Compiler,
    Source,
}

pub struct CompilerInterface {
    pub module: &'static str,
    pub implementation: InterfaceImplementation,
    pub values: &'static [(&'static str, Intrinsic)],
    pub types: &'static [(&'static str, TypeId)],
    pub classes: &'static [(&'static str, CompilerClass)],
}

pub const COMPILER_INTERFACES: &[CompilerInterface] = &[
    CompilerInterface {
        module: "Safe.Coerce",
        implementation: InterfaceImplementation::Compiler,
        values: &[("coerce", Intrinsic::Coerce)],
        types: &[("Coercible", TypeId::COERCIBLE)],
        classes: &[],
    },
    CompilerInterface {
        module: "Unsafe.Coerce",
        implementation: InterfaceImplementation::Compiler,
        values: &[("unsafeCoerce", Intrinsic::UnsafeCoerce)],
        types: &[],
        classes: &[],
    },
    CompilerInterface {
        module: "Data.Symbol",
        implementation: InterfaceImplementation::Source,
        values: &[],
        types: &[],
        classes: &[("IsSymbol", CompilerClass::IsSymbol)],
    },
];

pub fn compiler_interface(module: &str) -> Option<&'static CompilerInterface> {
    COMPILER_INTERFACES
        .iter()
        .find(|interface| interface.module == module)
}
