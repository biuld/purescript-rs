use super::convert::val_type;
use super::{
    Body, DataIndex, DataMode, DataSegment, Entry, Export, ExportIndex, ExportKind, FuncType,
    Function, FunctionIndex, Import, Memory, MemoryIndex, Module, Op, TypeIndex,
};
use crate::BackendError;
use crate::abi::{self, names};
use crate::capability::TargetCapabilities;
use crate::mir::{self, Function as MirFunction};
use crate::types::{DataId, HeapType, MemoryId, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;
use wasm_encoder::{Instruction, RefType, ValType};

mod asm;
mod codec;
mod extent;
mod function_types;
mod post_return;
mod realloc;
mod runtime;
mod structure;

use function_types::collect_function_types;
use realloc::build_realloc;
use runtime::{collect_literal_globals, collect_strings};
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
    mir::verify_module_with_capabilities(module, target)?;
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

    let (mut data, string_lengths) = collect_strings(module);
    let literal_globals = collect_literal_globals(module);
    if !literal_globals.globals.is_empty() && !target.mutable_globals {
        return Err(wasm_error(
            module.span,
            "string literal interning requires mutable WebAssembly globals",
        ));
    }
    extent::verify_static_access_extents(module)?;
    let needs_helpers = module.imports.iter().any(|import| {
        import.symbol == abi::STRING_TO_BYTES_SYMBOL || import.symbol == abi::BYTES_TO_STRING_SYMBOL
    });
    // The host allocates returned lists through `cabi_realloc`; indirect
    // parameter records and the string codec use the same allocator from inside
    // the guest.
    let needs_realloc = needs_helpers
        || module.imports.iter().any(|import| {
            import.symbol == abi::REALLOC_SYMBOL || wasi.has_list_result(import.symbol)
        });

    let type_defs = module.types.clone();
    let defined = type_defs
        .iter()
        .map(|group| group.0.len() as u32)
        .sum::<u32>();
    let (mut types, function_types) = collect_function_types(module, defined, Vec::new())?;

    // Core imports: the WASI imports MIR declared, then `exit-with-code` used
    // by the synthesized `run` entry when the target exposes WASI CLI.
    let mut imports = Vec::new();
    let mut import_indices = HashMap::<SymbolId, FunctionIndex>::new();
    for import in &module.imports {
        if matches!(
            import.symbol,
            abi::REALLOC_SYMBOL | abi::STRING_TO_BYTES_SYMBOL | abi::BYTES_TO_STRING_SYMBOL
        ) {
            continue;
        }
        let type_index = TypeIndex(defined + types.len() as u32);
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
        import_indices.insert(import.symbol, FunctionIndex(imports.len() as u32));
        imports.push(Import {
            module: module_name.to_string(),
            name: field.to_string(),
            type_index,
        });
    }
    // The synthesized `run` entry exits with `main`'s code through WASI. The
    // registry is the one P9 built; interning `exit` here does not add an import
    // unless a lowered call references it.
    let exit_index = if target.wasi_cli {
        let exit = wasi
            .import(names::EXIT, names::EXIT_WITH_CODE)
            .map_err(|message| {
                vec![BackendError::new(
                    "P10 Wasm structuring",
                    module.span,
                    message,
                )]
            })?;
        let exit_type_index = TypeIndex(defined + types.len() as u32);
        types.push(FuncType {
            parameters: exit.parameters.iter().map(|ty| val_type(*ty)).collect(),
            results: Vec::new(),
        });
        let index = FunctionIndex(imports.len() as u32);
        imports.push(Import {
            module: exit.module,
            name: exit.name,
            type_index: exit_type_index,
        });
        Some(index)
    } else {
        None
    };

    let import_count = imports.len() as u32;
    let entry_type = TypeIndex(defined + types.len() as u32);
    types.push(FuncType {
        parameters: Vec::new(),
        results: vec![ValType::I32],
    });

    let entry_index = FunctionIndex(import_count + module.functions.len() as u32);
    let indices = SynthesizedIndices::new(entry_index, needs_realloc, needs_helpers);

    let mut function_indices = module
        .functions
        .iter()
        .map(|function| (function.symbol, FunctionIndex(import_count + function.id.0)))
        .collect::<HashMap<_, _>>();
    for (symbol, index) in &import_indices {
        function_indices.insert(*symbol, *index);
    }
    if let Some(realloc) = indices.realloc {
        function_indices.insert(abi::REALLOC_SYMBOL, realloc);
    }
    // The GC string type index is carried by the reserved helper imports' value
    // types, so the synthesized codec names the same concrete type MIR does.
    let string_type = if needs_helpers {
        let string_type = string_type_from_imports(module)
            .ok_or_else(|| wasm_error(module.span, "the string codec has no GC string type"))?;
        function_indices.insert(
            abi::STRING_TO_BYTES_SYMBOL,
            indices
                .string_to_bytes
                .expect("a needed codec has a string_to_bytes index"),
        );
        function_indices.insert(
            abi::BYTES_TO_STRING_SYMBOL,
            indices
                .bytes_to_string
                .expect("a needed codec has a bytes_to_string index"),
        );
        Some(string_type)
    } else {
        None
    };

    let mut functions = Vec::with_capacity(module.functions.len());
    for (index, source) in module.functions.iter().enumerate() {
        let lowered = lower_function(
            source,
            function_types[index],
            &function_indices,
            &string_lengths,
            &literal_globals.indices,
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(source.symbol.module))
                .collect::<Vec<_>>()
        })?;
        functions.push(lowered);
    }

    let main_index = FunctionIndex(import_count + entry_function.id.0);

    // `cabi_realloc` backs guest allocations for indirect parameter records and
    // host allocations for returned lists/strings. Export it so the component
    // host can allocate returned buffers. The bump allocator's free pointer
    // lives after string data, and each allocation has a four-byte length
    // prefix before its returned payload pointer.
    let mut exports = vec![
        Export {
            name: abi::RUN_CORE_EXPORT.into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(entry_index),
        },
        Export {
            name: "memory".into(),
            kind: ExportKind::Memory,
            index: ExportIndex::Memory(MemoryIndex(0)),
        },
    ];
    let mut minimum = 1;
    let mut realloc = None;
    if needs_realloc {
        let realloc_type = TypeIndex(defined + types.len() as u32);
        types.push(FuncType {
            parameters: vec![ValType::I32; 4],
            results: vec![ValType::I32],
        });
        let index = indices.realloc.expect("a needed realloc has an index");
        exports.push(Export {
            name: "cabi_realloc".into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(index),
        });
        // The heap-state segment holds the free-list head (null) and the bump
        // break (the first allocatable address).
        let mut state = 0_u32.to_le_bytes().to_vec();
        state.extend_from_slice(&abi::HEAP_START.to_le_bytes());
        data.push(DataSegment {
            id: DataId(data.len() as u32),
            index: DataIndex(data.len() as u32),
            mode: DataMode::Active {
                offset: abi::HEAP_STATE,
            },
            bytes: state,
        });
        minimum = (u64::from(abi::HEAP_START)).div_ceil(0x10000) + 1;
        realloc = Some(build_realloc(realloc_type, module.span));
    }
    let helpers = if needs_helpers {
        let string_type = string_type.expect("a needed codec has a GC string type");
        let string_ref = val_type(ValueType::Ref(crate::types::RefType {
            nullable: false,
            heap: HeapType::Index(string_type),
        }));
        let (stb, bts, step) = codec::signatures(string_ref);
        let stb_type = TypeIndex(defined + types.len() as u32);
        types.push(stb);
        let bts_type = TypeIndex(defined + types.len() as u32);
        types.push(bts);
        let step_type = TypeIndex(defined + types.len() as u32);
        types.push(step);
        codec::synthesize(
            string_type,
            indices.realloc.expect("the codec needs the allocator"),
            indices.decode_step,
            stb_type,
            bts_type,
            step_type,
            module.span,
        )
    } else {
        Vec::new()
    };

    // `wasi:cli/run` returns a scalar, so the command export contributes no
    // owned-handle post-return. An export of `own<T>` is released here.
    debug_assert!(
        post_return::append_owned_handle_post_returns(
            &[],
            TypeIndex(0),
            FunctionIndex(0),
            module.span,
        )
        .is_empty()
    );

    let wasm = Module {
        name: module.name.clone(),
        imports,
        types,
        type_defs,
        functions,
        memories: vec![Memory {
            id: MemoryId(0),
            index: MemoryIndex(0),
            minimum,
            maximum: None,
        }],
        globals: literal_globals.globals,
        data,
        exports,
        entry: Some(Entry {
            type_index: entry_type,
            body: {
                let mut body = vec![Op::Leaf(Instruction::Call(main_index.0))];
                if let Some(exit_index) = exit_index {
                    body.push(Op::Leaf(Instruction::Call(exit_index.0)));
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                }
                body
            },
        }),
        realloc,
        helpers,
        span: module.span,
    };
    super::verify::verify_module(&wasm)?;
    Ok(wasm)
}

/// The final Wasm function indices of the synthesized helpers. The encoder
/// emits the entry, then `cabi_realloc`, then the codec helpers in their
/// declared order, so every index has exactly one source here.
struct SynthesizedIndices {
    realloc: Option<FunctionIndex>,
    string_to_bytes: Option<FunctionIndex>,
    bytes_to_string: Option<FunctionIndex>,
    decode_step: FunctionIndex,
}

impl SynthesizedIndices {
    fn new(entry: FunctionIndex, needs_realloc: bool, needs_helpers: bool) -> Self {
        let realloc = needs_realloc.then_some(FunctionIndex(entry.0 + 1));
        let helper_base = entry.0 + 1 + u32::from(needs_realloc);
        if needs_helpers {
            Self {
                realloc,
                string_to_bytes: Some(FunctionIndex(helper_base)),
                bytes_to_string: Some(FunctionIndex(helper_base + 1)),
                decode_step: FunctionIndex(helper_base + 2),
            }
        } else {
            Self {
                realloc,
                string_to_bytes: None,
                bytes_to_string: None,
                decode_step: FunctionIndex(helper_base),
            }
        }
    }
}

/// The GC string defined-type index, read from a reserved codec import's value
/// type. It is present exactly when the adapter needed a string transcode.
fn string_type_from_imports(module: &mir::Module) -> Option<crate::types::DefinedTypeId> {
    for import in &module.imports {
        if import.symbol != abi::STRING_TO_BYTES_SYMBOL
            && import.symbol != abi::BYTES_TO_STRING_SYMBOL
        {
            continue;
        }
        for ty in import.parameters.iter().chain(import.result.iter()) {
            if let ValueType::Ref(reference) = ty
                && let HeapType::Index(index) = reference.heap
            {
                return Some(index);
            }
        }
    }
    None
}

fn lower_function(
    source: &MirFunction,
    type_index: TypeIndex,
    function_indices: &HashMap<SymbolId, FunctionIndex>,
    string_lengths: &HashMap<crate::types::DataId, u32>,
    literal_globals: &HashMap<crate::types::DataId, crate::wasm::GlobalIndex>,
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
        .map(|value| defaultable_local_type(val_type(value.ty)))
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
        string_lengths,
        literal_globals,
    };
    let mut body = Body::new();
    let uses_dispatcher = structurer.emit_control_flow(&mut body)?;
    structurer.emit_load(source.result, source.span, &mut body)?;
    let mut locals = local_types;
    if uses_dispatcher {
        locals.push(ValType::I32);
    }
    Ok(Function {
        symbol: source.symbol,
        name: source.name.clone(),
        type_index,
        parameters,
        locals,
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

/// Wasm requires a non-defaultable (non-nullable reference) local to be
/// initialized in an enclosing scope before a read. The structurer writes MIR
/// block parameters inside nested structured blocks, so a non-parameter
/// reference local is declared nullable here and the structurer restores the
/// non-null type with `ref.as_non_null` at each read.
fn defaultable_local_type(ty: ValType) -> ValType {
    match ty {
        ValType::Ref(reference) if !reference.nullable => ValType::Ref(RefType {
            nullable: true,
            ..reference
        }),
        other => other,
    }
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
    vec![BackendError::invalid_ir(
        "P10 Wasm structuring",
        span,
        message,
    )]
}
