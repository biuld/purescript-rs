//! Normalize checked function aliases for the component's unique import names.
use std::collections::BTreeMap;
use wasm_encoder::reencode::{Error, Reencode};
use wasmparser::{Parser, Payload, TypeRef};

pub(crate) fn normalize(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut names = BTreeMap::new();
    let mut indices = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(section) = payload.map_err(|e| e.to_string())? {
            for import in section.into_imports() {
                let import = import.map_err(|e| e.to_string())?;
                if !matches!(import.ty, TypeRef::Func(_)) {
                    return Err(
                        "application import normalization requires checked functions".into(),
                    );
                }
                let next = names.len() as u32;
                let index = *names
                    .entry((import.module.to_owned(), import.name.to_owned()))
                    .or_insert(next);
                indices.push(index);
            }
        }
    }
    if indices.len() == names.len() {
        return Ok(bytes.to_vec());
    }
    let mut encoder = Aliases {
        original: indices.len() as u32,
        retained: names.len() as u32,
        indices,
    };
    let mut module = wasm_encoder::Module::new();
    encoder
        .parse_core_module(&mut module, Parser::new(0), bytes)
        .map_err(|e| e.to_string())?;
    let bytes = module.finish();
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

struct Aliases {
    original: u32,
    retained: u32,
    indices: Vec<u32>,
}

impl Reencode for Aliases {
    type Error = String;

    fn function_index(&mut self, index: u32) -> Result<u32, Error<String>> {
        Ok(if index < self.original {
            self.indices[index as usize]
        } else {
            index - self.original + self.retained
        })
    }

    fn parse_import_section(
        &mut self,
        imports: &mut wasm_encoder::ImportSection,
        section: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), Error<String>> {
        let mut seen = std::collections::BTreeSet::new();
        for import in section.into_imports() {
            let import = import?;
            if seen.insert((import.module, import.name)) {
                self.parse_import(imports, import)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_normalization_remaps_calls_exports_and_element_references() {
        let bytes = wat::parse_str(
            r#"(module
                (type $f (func (result i32)))
                (import "provider" "read" (func $a (type $f)))
                (import "provider" "read" (func $b (type $f)))
                (table 2 funcref)
                (elem (i32.const 0) func $b $run)
                (func $run (export "run") (type $f) call $b)
                (export "alias" (func $b)))"#,
        )
        .unwrap();
        let normalized = normalize(&bytes).unwrap();
        let mut imports = 0;
        let mut exports = BTreeMap::new();
        let mut elements = Vec::new();
        let mut calls = Vec::new();
        for payload in Parser::new(0).parse_all(&normalized) {
            match payload.unwrap() {
                Payload::ImportSection(section) => imports = section.count(),
                Payload::ExportSection(section) => {
                    for export in section {
                        let export = export.unwrap();
                        exports.insert(export.name.to_owned(), export.index);
                    }
                }
                Payload::ElementSection(section) => {
                    for element in section {
                        let wasmparser::ElementItems::Functions(functions) = element.unwrap().items
                        else {
                            panic!("expected function references");
                        };
                        elements.extend(functions.into_iter().map(Result::unwrap));
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    for operator in body.get_operators_reader().unwrap() {
                        if let wasmparser::Operator::Call { function_index } = operator.unwrap() {
                            calls.push(function_index);
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(imports, 1);
        assert_eq!(exports["run"], 1);
        assert_eq!(exports["alias"], 0);
        assert_eq!(elements, [0, 1]);
        assert_eq!(calls, [0]);
    }
}
