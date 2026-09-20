use super::convert::val_type;
use super::{
    Body, DataSegment, Entry, Export, ExportKind, FuncType, Function, Import, Memory, Module, Op,
};
use crate::BackendError;
use crate::abi::{self, names};
use crate::capability::TargetCapabilities;
use crate::mir::{self, Function as MirFunction};
use crate::types::{CompositeType, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{Instruction, MemArg, ValType};

mod capability;
mod runtime;
mod structure;

use capability::validate_target_capabilities;
use runtime::collect_strings;
use structure::Structurer;

/// Structures MIR control flow and builds the thin Wasm IR. The ABI registry
/// names each MIR import; MIR itself carries only canonical signatures.
pub fn lower_module(
    module: &mir::Module,
    wasi: &mut abi::WasiRegistry,
) -> Result<Module, Vec<BackendError>> {
    lower_module_with_capabilities(module, wasi, TargetCapabilities::default())
}

/// Lowers MIR using an explicit target capability profile.
pub fn lower_module_with_capabilities(
    module: &mir::Module,
    wasi: &mut abi::WasiRegistry,
    target: TargetCapabilities,
) -> Result<Module, Vec<BackendError>> {
    mir::verify_module(module)?;
    validate_target_capabilities(module, target)?;
    let Some(entry_symbol) = module.entry else {
        return Err(wasm_error(
            module.span,
            "no program entry point was selected for the component artifact",
        ));
    };
    let Some(main_position) = module
        .functions
        .iter()
        .position(|function| function.symbol == entry_symbol)
    else {
        return Err(wasm_error(
            module.span,
            "the program entry declaration is not a lowered function",
        ));
    };
    let entry_function = &module.functions[main_position];
    if !entry_function.parameters.is_empty() || entry_function.result_type != ValueType::I32 {
        return Err(wasm_error(
            entry_function.span,
            "the component artifact requires a zero-argument Int entry declaration",
        ));
    }

    let (string_offsets, mut data, data_end) = collect_strings(module);
    let needs_realloc = module
        .imports
        .iter()
        .any(|import| wasi.has_list_result(import.symbol));

    let defined = module
        .types
        .iter()
        .map(|group| group.0.len() as u32)
        .sum::<u32>();
    let (mut types, function_types) = collect_function_types(module, defined)?;

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
        let (module_name, field) = wasi.symbol_name(import.symbol).ok_or_else(|| {
            wasm_error(module.span, "a MIR import symbol has no ABI registry entry")
        })?;
        import_indices.insert(import.symbol, imports.len() as u32);
        imports.push(Import {
            module: module_name.to_string(),
            name: field.to_string(),
            type_index,
        });
    }
    // The synthesized `run` entry exits with `main`'s code through WASI. The
    // registry is the one P9 built; interning `exit` here does not add an import
    // unless a lowered call references it.
    let exit = wasi
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
        let lowered = lower_function(
            source,
            function_types[index],
            &function_indices,
            &string_offsets,
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(source.symbol.module))
                .collect::<Vec<_>>()
        })?;
        functions.push(lowered);
    }

    let main_index = import_count + main_position as u32;
    let entry_index = import_count + module.functions.len() as u32;

    // An imported function that returns a list/string has the host allocate the
    // buffer in guest memory, so the module must export `cabi_realloc`. The
    // allocator is a bump allocator whose free pointer lives in a data segment
    // after the string data. It prefixes each allocation with its length, so a
    // returned `(pointer, length)` can be a length-prefixed string value.
    let mut exports = vec![
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
    ];
    let mut minimum = 1;
    let mut realloc = None;
    if needs_realloc {
        let heap_pointer = data_end.next_multiple_of(4);
        let heap_start = (heap_pointer + 4).next_multiple_of(16);
        let realloc_type = defined + types.len() as u32;
        types.push(FuncType {
            parameters: vec![ValType::I32; 4],
            results: vec![ValType::I32],
        });
        let index = entry_index + 1;
        exports.push(Export {
            name: "cabi_realloc".into(),
            kind: ExportKind::Function,
            index,
        });
        data.push(DataSegment {
            offset: heap_pointer,
            bytes: heap_start.to_le_bytes().to_vec(),
        });
        minimum = (heap_start as u64).div_ceil(0x10000) + 1;
        realloc = Some(build_realloc(realloc_type, heap_pointer, module.span));
    }

    let wasm = Module {
        name: module.name.clone(),
        imports,
        types,
        type_defs: module.types.clone(),
        functions,
        memories: vec![Memory {
            minimum,
            maximum: None,
        }],
        data,
        exports,
        entry: Some(Entry {
            type_index: entry_type,
            body: vec![
                Op::Leaf(Instruction::Call(main_index)),
                Op::Leaf(Instruction::Call(exit_index)),
                Op::Leaf(Instruction::I32Const(0)),
            ],
        }),
        realloc,
        span: module.span,
    };
    super::verify::verify_module(&wasm)?;
    Ok(wasm)
}

/// Builds the bump-allocator `cabi_realloc` the canonical ABI calls to allocate
/// returned `list`/`string` buffers in guest memory. Each allocation is
/// preceded by a four-byte length so the result is a length-prefixed string
/// value. Old allocations are not reclaimed.
#[allow(clippy::vec_init_then_push)]
fn build_realloc(type_index: u32, heap_pointer: u32, span: TextRange) -> Function {
    let memarg = || MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    };
    let page_round = |body: &mut Body| {
        // (value + 65535) >> 16, the number of pages needed to hold `value`.
        body.push(Op::Leaf(Instruction::I32Const(65535)));
        body.push(Op::Leaf(Instruction::I32Add));
        body.push(Op::Leaf(Instruction::I32Const(16)));
        body.push(Op::Leaf(Instruction::I32ShrU));
    };
    let mut body = Body::new();
    // local 4 = free pointer
    body.push(Op::Leaf(Instruction::I32Const(heap_pointer as i32)));
    body.push(Op::Leaf(Instruction::I32Load(memarg())));
    body.push(Op::Leaf(Instruction::LocalSet(4)));
    // free pointer = (free + align - 1) & -align
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(2)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::I32Const(1)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::I32Const(0)));
    body.push(Op::Leaf(Instruction::LocalGet(2)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::I32And));
    body.push(Op::Leaf(Instruction::LocalSet(4)));
    // local 5 = end = free pointer + new size + the four-byte length prefix
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(3)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::LocalSet(5)));
    // Grow the memory if the allocation crosses the current size.
    body.push(Op::Leaf(Instruction::LocalGet(5)));
    page_round(&mut body);
    body.push(Op::Leaf(Instruction::MemorySize(0)));
    body.push(Op::Leaf(Instruction::I32GtU));
    let mut grow = Body::new();
    grow.push(Op::Leaf(Instruction::LocalGet(5)));
    page_round(&mut grow);
    grow.push(Op::Leaf(Instruction::MemorySize(0)));
    grow.push(Op::Leaf(Instruction::I32Sub));
    grow.push(Op::Leaf(Instruction::MemoryGrow(0)));
    grow.push(Op::Leaf(Instruction::Drop));
    body.push(Op::If {
        then_body: grow,
        else_body: Body::new(),
        result: None,
        span,
    });
    // Store the length prefix and the new free pointer.
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(3)));
    body.push(Op::Leaf(Instruction::I32Store(memarg())));
    body.push(Op::Leaf(Instruction::I32Const(heap_pointer as i32)));
    body.push(Op::Leaf(Instruction::LocalGet(5)));
    body.push(Op::Leaf(Instruction::I32Store(memarg())));
    // Return the buffer after its length prefix.
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32Add));
    Function {
        symbol: SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 1),
        name: "cabi_realloc".into(),
        type_index,
        parameters: vec![ValType::I32; 4],
        locals: vec![ValType::I32, ValType::I32],
        body,
        span,
    }
}

fn collect_function_types(
    module: &mir::Module,
    defined: u32,
) -> Result<(Vec<FuncType>, Vec<u32>), Vec<BackendError>> {
    let mut types = Vec::<FuncType>::new();
    let mut indices = HashMap::<(Vec<ValType>, Vec<ValType>), u32>::new();
    for (index, definition) in module
        .types
        .iter()
        .flat_map(|group| group.0.iter())
        .enumerate()
    {
        let CompositeType::Func {
            parameters,
            results,
        } = &definition.composite
        else {
            continue;
        };
        indices.insert(
            (
                parameters.iter().copied().map(val_type).collect(),
                results.iter().copied().map(val_type).collect(),
            ),
            index as u32,
        );
    }
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
                let index = defined + types.len() as u32;
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
