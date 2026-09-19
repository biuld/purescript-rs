use super::convert::val_type;
use super::{Body, Entry, Export, ExportKind, FuncType, Function, Import, Memory, Module, Op};
use crate::BackendError;
use crate::abi::{self, names};
use crate::mir::{self, Function as MirFunction};
use crate::types::{ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{Instruction, ValType};

mod runtime;
mod structure;

use runtime::collect_strings;
use structure::Structurer;

/// String data is placed after this scratch region, which holds the return
/// pointer for WASI calls and their results.
pub(super) const SCRATCH_END: u32 = 16;

/// Structures MIR control flow and builds the thin Wasm IR.
pub fn lower_module(module: &mir::Module) -> Result<Module, Vec<BackendError>> {
    mir::verify_module(module)?;
    let Some(main_position) = module.functions.iter().position(|function| {
        function.name == "main"
            && function.parameters.is_empty()
            && function.result_type == ValueType::I32
    }) else {
        return Err(wasm_error(
            module.span,
            "the component artifact requires a zero-argument Int `main` declaration",
        ));
    };

    let (string_offsets, data) = collect_strings(module);

    let (mut types, function_types) = collect_function_types(module)?;
    let defined = module
        .types
        .iter()
        .map(|group| group.0.len() as u32)
        .sum::<u32>();

    // Core imports: the WASI imports MIR declared, then `exit-with-code` used
    // by the synthesized `run` entry.
    let mut imports = Vec::new();
    let mut import_indices = HashMap::new();
    for import in &module.imports {
        let type_index = defined + types.len() as u32;
        types.push(FuncType {
            parameters: import.parameters.iter().map(|ty| val_type(*ty)).collect(),
            results: import
                .result
                .map(|ty| vec![val_type(ty)])
                .unwrap_or_default(),
        });
        import_indices.insert(import.symbol, imports.len() as u32);
        imports.push(Import {
            module: import.module.clone(),
            name: import.name.clone(),
            type_index,
        });
    }
    // The synthesized `run` entry exits with `main`'s code through WASI.
    let mut registry = abi::WasiRegistry::load().map_err(|message| {
        vec![BackendError::new(
            "P10 Wasm structuring",
            module.span,
            message,
        )]
    })?;
    let exit = registry
        .import(names::EXIT, names::EXIT_WITH_CODE)
        .map_err(|message| {
            vec![BackendError::new(
                "P10 Wasm structuring",
                module.span,
                message,
            )]
        })?;
    let exit_type_index = defined + types.len() as u32;
    types.push(FuncType {
        parameters: exit.parameters.iter().map(|ty| val_type(*ty)).collect(),
        results: Vec::new(),
    });
    let exit_index = imports.len() as u32;
    imports.push(Import {
        module: exit.module,
        name: exit.name,
        type_index: exit_type_index,
    });

    let import_count = imports.len() as u32;
    let entry_type = defined + types.len() as u32;
    types.push(FuncType {
        parameters: Vec::new(),
        results: vec![ValType::I32],
    });

    let mut function_indices = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.symbol, import_count + index as u32))
        .collect::<HashMap<_, _>>();
    for (symbol, index) in &import_indices {
        function_indices.insert(*symbol, *index);
    }

    let mut functions = Vec::with_capacity(module.functions.len());
    for (index, source) in module.functions.iter().enumerate() {
        functions.push(lower_function(
            source,
            defined + function_types[index],
            &function_indices,
            &string_offsets,
        )?);
    }

    let main_index = import_count + main_position as u32;
    let entry_index = import_count + module.functions.len() as u32;

    let wasm = Module {
        name: module.name.clone(),
        imports,
        types,
        type_defs: module.types.clone(),
        functions,
        memories: vec![Memory {
            minimum: 1,
            maximum: None,
        }],
        data,
        exports: vec![
            Export {
                name: abi::RUN_CORE_EXPORT.into(),
                kind: ExportKind::Function,
                index: entry_index,
            },
            Export {
                name: "memory".into(),
                kind: ExportKind::Memory,
                index: 0,
            },
        ],
        entry: Some(Entry {
            type_index: entry_type,
            body: vec![
                Op::Leaf(Instruction::Call(main_index)),
                Op::Leaf(Instruction::Call(exit_index)),
                Op::Leaf(Instruction::I32Const(0)),
            ],
        }),
        span: module.span,
    };
    super::verify::verify_module(&wasm)?;
    Ok(wasm)
}

fn collect_function_types(
    module: &mir::Module,
) -> Result<(Vec<FuncType>, Vec<u32>), Vec<BackendError>> {
    let mut types = Vec::<FuncType>::new();
    let mut indices = HashMap::<(Vec<ValType>, Vec<ValType>), u32>::new();
    let mut function_types = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        if function.parameters.len() > function.values.len() {
            return Err(wasm_error(
                function.span,
                "MIR function has more parameters than values",
            ));
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let ty = value_type(function, *parameter).ok_or_else(|| {
                wasm_error(function.span, "MIR function parameter has no value type")
            })?;
            parameters.push(val_type(ty));
        }
        let key = (parameters, vec![val_type(function.result_type)]);
        let type_index = match indices.get(&key) {
            Some(index) => *index,
            None => {
                let index = types.len() as u32;
                types.push(FuncType {
                    parameters: key.0.clone(),
                    results: key.1.clone(),
                });
                indices.insert(key, index);
                index
            }
        };
        function_types.push(type_index);
    }
    Ok((types, function_types))
}

fn lower_function(
    source: &MirFunction,
    type_index: u32,
    function_indices: &HashMap<SymbolId, u32>,
    string_offsets: &HashMap<String, u32>,
) -> Result<Function, Vec<BackendError>> {
    let locals = local_indices(source)?;
    let parameters = source
        .parameters
        .iter()
        .map(|parameter| {
            value_type(source, *parameter)
                .map(val_type)
                .ok_or_else(|| wasm_error(source.span, "MIR function parameter has no value type"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let local_types = source
        .values
        .iter()
        .skip(source.parameters.len())
        .map(|value| val_type(value.ty))
        .collect::<Vec<_>>();
    let structurer = Structurer {
        function: source,
        blocks: source
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect(),
        locals,
        function_indices,
        string_offsets,
    };
    let mut body = Body::new();
    structurer.emit_region(source.entry, None, &mut HashSet::new(), &mut body)?;
    body.push(Op::Leaf(Instruction::LocalGet(local(
        &structurer.locals,
        source.result,
        source.span,
    )?)));
    Ok(Function {
        symbol: source.symbol,
        name: source.name.clone(),
        type_index,
        parameters,
        locals: local_types,
        body,
        span: source.span,
    })
}

fn local_indices(function: &MirFunction) -> Result<HashMap<ValueId, u32>, Vec<BackendError>> {
    let locals = function
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| (value.id, index as u32))
        .collect::<HashMap<_, _>>();
    for (index, parameter) in function.parameters.iter().enumerate() {
        if locals.get(parameter).copied() != Some(index as u32) {
            return Err(wasm_error(
                function.span,
                "MIR parameters must occupy the first Wasm local indices",
            ));
        }
    }
    Ok(locals)
}

pub(super) fn local(
    locals: &HashMap<ValueId, u32>,
    value: ValueId,
    span: TextRange,
) -> Result<u32, Vec<BackendError>> {
    locals
        .get(&value)
        .copied()
        .ok_or_else(|| wasm_error(span, "MIR value has no Wasm local index"))
}

fn value_type(function: &MirFunction, value: ValueId) -> Option<ValueType> {
    function
        .values
        .iter()
        .find(|declaration| declaration.id == value)
        .map(|declaration| declaration.ty)
}

pub(super) fn wasm_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P10 Wasm structuring", span, message)]
}
