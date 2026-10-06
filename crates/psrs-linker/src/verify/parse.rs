//! Parsing a core module and comparing it with a declared contract.

use crate::error::{LinkErrors, LinkStage};
use crate::target::{
    ArtifactContract, CoreSignature, CoreType, DeclaredExport, ExportKind, ImportKind,
};
use wasmparser::{CompositeInnerType, ExternalKind, Operator, Parser, Payload, TypeRef, ValType};

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
    if parsed.has_elements {
        return Err(LinkErrors::one(
            stage,
            id,
            "artifact element segments are not declared",
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
    has_elements: bool,
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
    let mut types = Vec::new();
    let mut imported_functions = Vec::new();
    let mut defined_functions = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut tables = Vec::new();
    let mut globals = Vec::new();
    let mut data_ranges = Vec::new();
    let mut memories = 0_u32;
    let mut has_start = false;
    let mut has_elements = false;

    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|error| error.to_string())? {
            Payload::TypeSection(reader) => {
                for group in reader {
                    for ty in group.map_err(|error| error.to_string())?.into_types() {
                        if let CompositeInnerType::Func(func) = ty.composite_type.inner {
                            types.push(func);
                        } else {
                            return Err("artifact defines a non-function composite type".into());
                        }
                    }
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
                                kind: ImportKind::Memory,
                            });
                        }
                        TypeRef::Func(index) => {
                            imported_functions.push(index);
                            imports.push(ParsedImport {
                                module: import.module.to_string(),
                                field: import.name.to_string(),
                                kind: ImportKind::Function,
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
            Payload::ElementSection(_) => has_elements = true,
            _ => {}
        }
    }
    if memories > 0 {
        return Err("artifact defines its own memory instead of importing one".into());
    }
    if !imported_functions.is_empty() {
        return Err("artifact function imports are not supported".into());
    }
    Ok(ParsedCore {
        imports,
        exports,
        tables,
        globals,
        data_ranges,
        has_start,
        has_elements,
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
    types: &[wasmparser::FuncType],
) -> Result<CoreSignature, String> {
    let type_index = if (index as usize) < imported_functions.len() {
        imported_functions[index as usize]
    } else {
        let defined = index as usize - imported_functions.len();
        *defined_functions
            .get(defined)
            .ok_or("artifact export refers to an unknown function")?
    };
    let ty = types
        .get(type_index as usize)
        .ok_or("artifact function has no type")?;
    let convert = |ty: ValType| match ty {
        ValType::I32 => Some(CoreType::I32),
        ValType::I64 => Some(CoreType::I64),
        ValType::F32 => Some(CoreType::F32),
        ValType::F64 => Some(CoreType::F64),
        _ => None,
    };
    let mut parameters = Vec::with_capacity(ty.params().len());
    for param in ty.params() {
        parameters.push(convert(*param).ok_or("artifact parameter is not a scalar")?);
    }
    let result = match ty.results() {
        [] => None,
        [single] => Some(convert(*single).ok_or("artifact result is not a scalar")?),
        _ => return Err("artifact function has multiple results".into()),
    };
    Ok(CoreSignature { parameters, result })
}
