mod resolver;

pub use resolver::{
    ResolveError, ResolveErrorKind, bootstrap_intrinsics, resolve_module,
    resolve_module_with_externals,
};
