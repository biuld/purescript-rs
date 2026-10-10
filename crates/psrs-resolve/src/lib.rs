mod resolver;

pub use resolver::{
    ProgramError, ResolveError, ResolveErrorKind, ResolveOptions, bootstrap_externals,
    compiler_provided_module, resolve_module, resolve_module_with_externals, resolve_program,
    resolve_program_partial, resolve_program_with_options,
};
