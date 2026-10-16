//! Derive the component tooling's executable import closure from planned calls.
//!
//! A live resource alias can require its defining interface, while unused
//! functions in that interface do not introduce their own type dependencies.
//! The encoder owns this projection; walking all WIT types over-approximates it.
use crate::plan::ResolvedBinding;
use crate::{CoreType, LinkErrors, LinkStage, ResolvedWorldContext};
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemorySection, MemoryType, Module, TypeSection, ValType,
};

pub(crate) fn host_imports(
    context: &ResolvedWorldContext,
    bindings: &[ResolvedBinding],
) -> Result<Vec<String>, LinkErrors> {
    let hosts: Vec<_> = bindings
        .iter()
        .filter(|binding| context.imports_interface(&binding.module))
        .collect();
    let Some(world) = context.composition_world() else {
        let mut imports: Vec<_> = hosts.iter().map(|binding| binding.module.clone()).collect();
        imports.sort();
        imports.dedup();
        return Ok(imports);
    };
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let mut module = Module::new();
    let mut types = TypeSection::new();
    let mut imports = ImportSection::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut count = 0_u32;
    for binding in hosts {
        if !seen.insert((&binding.module, &binding.field)) {
            continue;
        }
        types.ty().function(
            binding
                .signature
                .parameters
                .iter()
                .map(scalar)
                .collect::<Result<Vec<_>, _>>()?,
            binding.signature.result.as_ref().map(scalar).transpose()?,
        );
        imports.import(&binding.module, &binding.field, EntityType::Function(count));
        count += 1;
    }
    types.ty().function([ValType::I32; 4], [ValType::I32]);
    module.section(&types);
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(count);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("cabi_realloc", ExportKind::Func, count);
    module.section(&exports);
    let mut function = Function::new([]);
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    // This module is a nonexecuted signature projection, not an implementation.
    // Export requirements are irrelevant to the import-only projection.
    let mut resolve = context.resolve().clone();
    resolve.worlds[world].exports.clear();
    let mut bytes = module.finish();
    wit_component::embed_component_metadata(
        &mut bytes,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .map_err(error)?;
    let component = wit_component::ComponentEncoder::default()
        .module(&bytes)
        .map_err(error)?
        .validate(true)
        .encode()
        .map_err(error)?;
    crate::compose::component_imports(&component)
}

fn scalar(ty: &CoreType) -> Result<ValType, LinkErrors> {
    Ok(match ty {
        CoreType::I32 => ValType::I32,
        CoreType::I64 => ValType::I64,
        CoreType::F32 => ValType::F32,
        CoreType::F64 => ValType::F64,
        CoreType::V128 | CoreType::Ref(_) => {
            return Err(error(
                "a raw GC or SIMD contract cannot cross a WIT scalar boundary",
            ));
        }
    })
}
fn error(error: impl std::fmt::Display) -> LinkErrors {
    LinkErrors::plain(
        LinkStage::Requirements,
        format!("planned WIT call projection is invalid: {error}"),
    )
}
