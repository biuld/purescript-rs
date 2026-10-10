//! Explicit conversion of MIR type arenas to linker-owned raw contracts.
use crate::{mir, types::RecGroup};
use psrs_linker::{CoreSignature, CoreTypes};

pub(crate) fn raw_signature(
    import: &mir::Import,
    groups: &[RecGroup],
) -> Result<CoreSignature, String> {
    let mut types = wasm_encoder::TypeSection::new();
    for group in groups {
        types.ty().rec(group.0.iter().map(super::convert::sub_type));
    }
    let signature = groups.iter().map(|group| group.0.len()).sum::<usize>() as u32;
    types.ty().function(
        import
            .parameters
            .iter()
            .copied()
            .map(super::convert::val_type),
        import.result.into_iter().map(super::convert::val_type),
    );
    let mut schema = wasm_encoder::Module::new();
    schema.section(&types);
    CoreTypes::from_module(&schema.finish())?.signature(signature)
}
