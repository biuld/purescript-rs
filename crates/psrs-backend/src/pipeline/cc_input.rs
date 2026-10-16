//! The shared target pipeline is also usable from its defined CC boundary.

use crate::{Artifact, BackendError, BackendInput, TargetCapabilities, annotate_errors, cc, mir};

pub(crate) fn compile(input: BackendInput) -> Result<Artifact, Vec<BackendError>> {
    let target = TargetCapabilities::default();
    let owner = input.cc.entry.map(|entry| entry.module);
    cc::verify_module(&input.cc).map_err(|errors| annotate_errors(errors, owner))?;
    let context = crate::linking::default_context()?;
    let (mir, mut wasi) = mir::lower_module_with_bindings_and_resolve(
        input.cc,
        input.externals,
        target,
        context.shared_resolve(),
    )?;
    let mir = mir::opt::optimize(mir, target)?;
    let emitted = super::emission::emit(&mir, &mut wasi, target, owner, None, None, None)?;
    Ok(Artifact {
        wasm: emitted.component,
        wat: emitted.wat,
        warnings: input.warnings,
    })
}
