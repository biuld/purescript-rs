//! Parsing a core module and comparing it with a declared contract.

use crate::error::{LinkErrors, LinkStage};
use crate::target::{ArtifactContract, CoreSignature, DeclaredExport, ExportKind, ImportKind};
use wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef, ValType};

pub(super) fn check_contract(
    contract: &ArtifactContract,
    parsed: &ParsedCore,
) -> Result<(), LinkErrors> {
    let stage = LinkStage::Verify;
    let id = &contract.id;

    if parsed.imports.len() != contract.imports.len()
        || parsed
            .imports
            .iter()
            .zip(&contract.imports)
            .any(|(a, b)| a.module != b.module || a.field != b.field || a.kind != b.kind)
    {
        return Err(LinkErrors::one(
            stage,
            id,
            "artifact imports do not match the declared contract",
        ));
    }

    let mut declared_exports: Vec<&DeclaredExport> = contract.exports.iter().collect();
    for export in &parsed.exports {
        let Some(index) = declared_exports
            .iter()
            .position(|declared| declared.name == export.name && declared.kind == export.kind)
        else {
            return Err(LinkErrors::one(
                stage,
                id,
                format!("artifact has an undeclared export `{}`", export.name),
            ));
        };
        let declared = declared_exports.swap_remove(index);
        if declared.kind == ExportKind::Func && declared.signature != export.signature {
            return Err(LinkErrors::one(
                stage,
                id,
                format!("export `{}` has an unexpected signature", export.name),
            ));
        }
    }
    if let Some(missing) = declared_exports.first() {
        return Err(LinkErrors::one(
            stage,
            id,
            format!("artifact is missing declared export `{}`", missing.name),
        ));
    }

    if tables_differ(parsed, contract) {
        return Err(LinkErrors::one(
            stage,
            id,
            "artifact tables do not match the declared contract",
        ));
    }
    if globals_differ(parsed, contract) {
        return Err(LinkErrors::one(
            stage,
            id,
            "artifact globals do not match the declared contract",
        ));
    }
    if let Some(storage) = &contract.storage {
        let absolute = storage.stack_pointer_global as usize;
        let defined = absolute
            .checked_sub(parsed.imported_globals)
            .ok_or_else(|| LinkErrors::one(stage, id, "stack pointer global is an import"))?;
        let pointer = parsed
            .globals
            .get(defined)
            .ok_or_else(|| LinkErrors::one(stage, id, "stack pointer global is absent"))?;
        if !pointer.mutable
            || pointer.initial != storage.stack.end
            || storage.stack.start >= storage.stack.end
            || storage.static_data.start > storage.static_data.end
            || contract.initialization.data_range.0 < storage.static_data.start
            || contract.initialization.data_range.1 > storage.static_data.end
            || storage.stack_bound_bytes > storage.stack.end - storage.stack.start
        {
            return Err(LinkErrors::one(
                stage,
                id,
                "execution storage disagrees with the artifact stack pointer or initialization",
            ));
        }
    }
    if parsed.elements != contract.elements {
        return Err(LinkErrors::one(
            stage,
            id,
            "artifact element segments do not match the declared contract",
        ));
    }
    for (start, length) in &parsed.data_ranges {
        let end = start
            .checked_add(*length)
            .ok_or_else(|| LinkErrors::one(stage, id, "artifact data range overflows"))?;
        let (low, high) = contract.initialization.data_range;
        if *start < low || end > high {
            return Err(LinkErrors::one(
                stage,
                id,
                "artifact data outside its declared initialization range",
            ));
        }
    }
    if contract.initialization.start_forbidden && parsed.has_start {
        return Err(LinkErrors::one(
            stage,
            id,
            "artifact defines a start function, which its contract forbids",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct ParsedImport {
    module: String,
    field: String,
    kind: ImportKind,
}

struct ParsedExport {
    name: String,
    kind: ExportKind,
    signature: Option<CoreSignature>,
}

struct ParsedTable {
    element: String,
    minimum: u32,
    maximum: Option<u32>,
}

struct ParsedGlobal {
    mutable: bool,
    initial: u32,
}

pub(super) struct ParsedCore {
    imports: Vec<ParsedImport>,
    exports: Vec<ParsedExport>,
    tables: Vec<ParsedTable>,
    globals: Vec<ParsedGlobal>,
    data_ranges: Vec<(u32, u32)>,
    has_start: bool,
    elements: Vec<crate::DeclaredElement>,
    imported_globals: usize,
}

impl ParsedTable {
    fn matches(&self, declared: &crate::target::DeclaredTable) -> bool {
        self.element == declared.element
            && self.minimum == declared.minimum
            && self.maximum == declared.maximum
    }
}

fn tables_differ(parsed: &ParsedCore, contract: &ArtifactContract) -> bool {
    parsed.tables.len() != contract.tables.len()
        || parsed
            .tables
            .iter()
            .zip(&contract.tables)
            .any(|(a, b)| !a.matches(b))
}

fn globals_differ(parsed: &ParsedCore, contract: &ArtifactContract) -> bool {
    parsed.globals.len() != contract.globals.len()
        || parsed
            .globals
            .iter()
            .zip(&contract.globals)
            .any(|(a, b)| a.mutable != b.mutable || a.initial != b.initial)
}

pub(super) fn parse_core_module(bytes: &[u8]) -> Result<ParsedCore, String> {
    let mut types = crate::CoreTypes::default();
    let mut imported_functions = Vec::new();
    let mut defined_functions = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut tables = Vec::new();
    let mut globals = Vec::new();
    let mut data_ranges = Vec::new();
    let mut memories = 0_u32;
    let mut has_start = false;
    let mut elements = Vec::new();
    let mut imported_globals = 0_usize;

    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|error| error.to_string())? {
            Payload::TypeSection(reader) => {
                for group in reader {
                    types.push(group.map_err(|error| error.to_string())?)?;
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|error| error.to_string())?;
                    match import.ty {
                        TypeRef::Memory(memory) => {
                            if memory.memory64 || memory.shared {
                                return Err("artifact imports an unsupported memory".into());
                            }
                            imports.push(ParsedImport {
                                module: import.module.to_string(),
                                field: import.name.to_string(),
                                kind: ImportKind::Memory {
                                    minimum: memory.initial,
                                    maximum: memory.maximum,
                                },
                            });
                        }
                        TypeRef::Global(global) => {
                            if global.content_type != ValType::I32 || global.shared {
                                return Err("artifact imports an unsupported global".into());
                            }
                            imported_globals += 1;
                            imports.push(ParsedImport {
                                module: import.module.to_string(),
                                field: import.name.to_string(),
                                kind: ImportKind::Global {
                                    mutable: global.mutable,
                                },
                            });
                        }
                        TypeRef::Func(index) => {
                            imported_functions.push(index);
                            imports.push(ParsedImport {
                                module: import.module.to_string(),
                                field: import.name.to_string(),
                                kind: ImportKind::Function(function_signature(
                                    imported_functions.len() as u32 - 1,
                                    &imported_functions,
                                    &[],
                                    &types,
                                )?),
                            });
                        }
                        _ => return Err("artifact imports an unsupported item".into()),
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for ty in reader {
                    defined_functions.push(ty.map_err(|error| error.to_string())?);
                }
            }
            Payload::TableSection(reader) => {
                for table in reader {
                    let table = table.map_err(|error| error.to_string())?;
                    let element = if table.ty.element_type == wasmparser::RefType::FUNCREF {
                        "funcref"
                    } else if table.ty.element_type == wasmparser::RefType::EXTERNREF {
                        "externref"
                    } else {
                        return Err("artifact defines an unsupported table element type".into());
                    };
                    tables.push(ParsedTable {
                        element: element.into(),
                        minimum: table.ty.initial as u32,
                        maximum: table.ty.maximum.map(|value| value as u32),
                    });
                }
            }
            Payload::GlobalSection(reader) => {
                for global in reader {
                    let global = global.map_err(|error| error.to_string())?;
                    if global.ty.content_type != ValType::I32 || global.ty.shared {
                        return Err("artifact defines an unsupported global".into());
                    }
                    let initial = constant_i32(&global.init_expr)?;
                    globals.push(ParsedGlobal {
                        mutable: global.ty.mutable,
                        initial,
                    });
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|error| error.to_string())?;
                    match export.kind {
                        ExternalKind::Func => {
                            let signature = function_signature(
                                export.index,
                                &imported_functions,
                                &defined_functions,
                                &types,
                            )?;
                            exports.push(ParsedExport {
                                name: export.name.to_string(),
                                kind: ExportKind::Func,
                                signature: Some(signature),
                            });
                        }
                        ExternalKind::Global => exports.push(ParsedExport {
                            name: export.name.to_string(),
                            kind: ExportKind::Global,
                            signature: None,
                        }),
                        _ => {
                            return Err(format!(
                                "artifact export `{}` has an undeclared kind",
                                export.name
                            ));
                        }
                    }
                }
            }
            Payload::DataSection(reader) => {
                for data in reader {
                    let data = data.map_err(|error| error.to_string())?;
                    let wasmparser::DataKind::Active {
                        memory_index: 0,
                        offset_expr,
                    } = data.kind
                    else {
                        return Err("artifact has unsupported or passive data storage".into());
                    };
                    let offset = constant_i32(&offset_expr)?;
                    data_ranges.push((offset, data.data.len() as u32));
                }
            }
            Payload::MemorySection(_) => memories += 1,
            Payload::StartSection { .. } => has_start = true,
            Payload::ElementSection(reader) => {
                for element in reader {
                    let element = element.map_err(|error| error.to_string())?;
                    let wasmparser::ElementKind::Active {
                        table_index,
                        offset_expr,
                    } = element.kind
                    else {
                        return Err(
                            "artifact has unsupported passive or declarative elements".into()
                        );
                    };
                    let wasmparser::ElementItems::Functions(functions) = element.items else {
                        return Err("artifact has unsupported element expressions".into());
                    };
                    elements.push(crate::DeclaredElement {
                        table: table_index.unwrap_or(0),
                        offset: constant_i32(&offset_expr)?,
                        functions: functions
                            .into_iter()
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|error| error.to_string())?,
                    });
                }
            }
            _ => {}
        }
    }
    if memories > 0 {
        return Err("artifact defines its own memory instead of importing one".into());
    }
    Ok(ParsedCore {
        imports,
        exports,
        tables,
        globals,
        data_ranges,
        has_start,
        elements,
        imported_globals,
    })
}

fn constant_i32(expr: &wasmparser::ConstExpr<'_>) -> Result<u32, String> {
    let mut ops = expr.get_operators_reader();
    let value = match ops.read().map_err(|error| error.to_string())? {
        Operator::I32Const { value } => value as u32,
        _ => return Err("artifact constant is not an i32".into()),
    };
    if !matches!(
        ops.read().map_err(|error| error.to_string())?,
        Operator::End
    ) {
        return Err("artifact constant has extra operators".into());
    }
    Ok(value)
}

fn function_signature(
    index: u32,
    imported_functions: &[u32],
    defined_functions: &[u32],
    types: &crate::CoreTypes,
) -> Result<CoreSignature, String> {
    let type_index = if (index as usize) < imported_functions.len() {
        imported_functions[index as usize]
    } else {
        let defined = index as usize - imported_functions.len();
        *defined_functions
            .get(defined)
            .ok_or("artifact export refers to an unknown function")?
    };
    types.signature(type_index)
}
