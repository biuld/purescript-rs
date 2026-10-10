//! Interface-only WIT input; original application instructions are never replaced.
use super::*;
use wasm_encoder::{ElementSection, ExportSection, GlobalSection, ImportSection};

struct Indices(Vec<u32>);
impl Reencode for Indices {
    type Error = String;
    fn function_index(&mut self, index: u32) -> Result<u32, Error<String>> {
        Ok(self.0.get(index as usize).copied().unwrap_or(index))
    }
}

/// Preserve permitted host and registered canonical library imports while
/// replacing raw provider imports with local declarations. Import permutation
/// is reflected in all function references.
pub(super) fn boundary_module(bytes: &[u8], hosts: &[String]) -> Result<Vec<u8>, String> {
    let mut imports = ImportSection::new();
    let mut private = Vec::new();
    let mut functions = FunctionSection::new();
    let mut permutation = Vec::new();
    let mut host_count = 0;
    let mut has_imports = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ImportSection(section) => {
                has_imports = true;
                for import in section.into_imports() {
                    let import = import.map_err(|e| e.to_string())?;
                    let TypeRef::Func(index) = import.ty else {
                        return Err(
                            "canonical boundary projection requires application-owned storage"
                                .into(),
                        );
                    };
                    if hosts.iter().any(|host| host == import.module) {
                        imports.import(
                            import.module,
                            import.name,
                            wasm_encoder::EntityType::Function(index),
                        );
                        permutation.push((true, host_count));
                        host_count += 1;
                    } else {
                        permutation.push((false, private.len() as u32));
                        private.push(index);
                    }
                }
                for index in &private {
                    functions.function(*index);
                }
            }
            Payload::FunctionSection(section) => {
                for index in section {
                    functions.function(index.map_err(|e| e.to_string())?);
                }
            }
            Payload::CustomSection(section) if section.name() == MARKER => {
                return Err("source module uses a reserved canonical boundary marker".into());
            }
            Payload::StartSection { .. } => {
                return Err(
                    "canonical boundary cannot project an application start function".into(),
                );
            }
            _ => {}
        }
    }
    let mut indices = Indices(
        permutation
            .into_iter()
            .map(|(host, index)| if host { index } else { host_count + index })
            .collect(),
    );
    let mut module = Module::new();
    let mut code = CodeSection::new();
    for _ in 0..functions.len() {
        let mut body = Function::new([]);
        body.instruction(&Instruction::Unreachable);
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    let mut code_emitted = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ImportSection(_) => {
                if !imports.is_empty() {
                    module.section(&imports);
                }
                if !functions.is_empty() {
                    module.section(&functions);
                }
            }
            Payload::FunctionSection(_)
            | Payload::CodeSectionEntry(_)
            | Payload::Version { .. } => {}
            Payload::CodeSectionStart { .. } => {
                module.section(&code);
                code_emitted = true;
            }
            Payload::ExportSection(section) => {
                let mut exports = ExportSection::new();
                indices
                    .parse_export_section(&mut exports, section)
                    .map_err(|e| e.to_string())?;
                module.section(&exports);
            }
            Payload::GlobalSection(section) => {
                let mut globals = GlobalSection::new();
                indices
                    .parse_global_section(&mut globals, section)
                    .map_err(|e| e.to_string())?;
                module.section(&globals);
            }
            Payload::ElementSection(section) => {
                let mut elements = ElementSection::new();
                indices
                    .parse_element_section(&mut elements, section)
                    .map_err(|e| e.to_string())?;
                module.section(&elements);
            }
            Payload::End(_) => {
                if !code_emitted && !functions.is_empty() {
                    module.section(&code);
                }
            }
            payload => {
                if let Some((id, range)) = payload.as_section() {
                    module.section(&RawSection {
                        id,
                        data: &bytes[range],
                    });
                    if id == 1 && !has_imports && !functions.is_empty() {
                        module.section(&functions);
                    }
                }
            }
        }
    }
    module.section(&CustomSection {
        name: Cow::Borrowed(MARKER),
        data: Cow::Borrowed(b"1"),
    });
    let bytes = module.finish();
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
