mod typecheck;

pub use typecheck::{
    TypeCheckError, TypeCheckErrorKind, typecheck_module, typecheck_module_with_imports,
    typecheck_module_with_imports_and_effect_representation,
};
