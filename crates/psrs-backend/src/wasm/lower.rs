use super::convert::val_type;
use super::{
    Body, DataSegment, Entry, Export, ExportKind, FuncType, Function, Import, Memory, Module, Op,
    RuntimeFunction,
};
use crate::BackendError;
use crate::cc::{ValueId, ValueType};
use crate::mir::{
    self, BlockId, Function as MirFunction, Instruction as MirInstruction, Terminator,
};
use psrs_core::Primitive;
use psrs_hir::{ExternalKind, RuntimeFunction as HirRuntimeFunction, SymbolId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{Instruction, MemArg, ValType};

/// Runtime scratch layout: a single-entry iovec at `0`, the bytes-written cell
/// at `8`, and the newline byte appended by `log` at `12`. String data is
/// placed after this region.
const NWRITTEN_ADDR: u32 = 8;
const NEWLINE_ADDR: u32 = 12;
const SCRATCH_END: u32 = 16;

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
            "the first Wasm artifact requires a zero-argument Int `main` declaration",
        ));
    };

    let (string_offsets, mut data) = collect_strings(module);
    let log_symbol = console_log_symbol(module);
    let log_used = log_symbol.is_some_and(|symbol| {
        module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| {
                matches!(instruction, MirInstruction::Call { function, .. } if *function == symbol)
            })
    });
    if log_used {
        data.push(DataSegment {
            offset: NEWLINE_ADDR,
            bytes: vec![b'\n'],
        });
    }

    let (mut types, function_types) = collect_function_types(module)?;

    // Import 0: exit with a status code, used by the synthesized entry.
    let proc_exit_type = types.len() as u32;
    types.push(FuncType {
        parameters: vec![ValType::I32],
        results: Vec::new(),
    });
    let mut imports = vec![Import {
        module: "wasi_snapshot_preview1".into(),
        name: "proc_exit".into(),
        type_index: proc_exit_type,
    }];

    let mut fd_write_index = None;
    let mut log_type = None;
    if log_used {
        let fd_write_type = types.len() as u32;
        types.push(FuncType {
            parameters: vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            results: vec![ValType::I32],
        });
        fd_write_index = Some(imports.len() as u32);
        imports.push(Import {
            module: "wasi_snapshot_preview1".into(),
            name: "fd_write".into(),
            type_index: fd_write_type,
        });

        let console_log_type = types.len() as u32;
        types.push(FuncType {
            parameters: vec![ValType::I32],
            results: vec![ValType::I32],
        });
        log_type = Some(console_log_type);
    }

    let import_count = imports.len() as u32;
    let runtime_count = u32::from(log_used);

    let entry_type = types.len() as u32;
    types.push(FuncType {
        parameters: Vec::new(),
        results: Vec::new(),
    });

    let mut function_indices = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.symbol, import_count + index as u32))
        .collect::<HashMap<_, _>>();
    if let Some(index) = log_used.then_some(import_count + module.functions.len() as u32) {
        function_indices.insert(log_symbol.expect("log is an external symbol"), index);
    }

    let mut functions = Vec::with_capacity(module.functions.len());
    for (index, source) in module.functions.iter().enumerate() {
        functions.push(lower_function(
            source,
            function_types[index],
            &function_indices,
            &string_offsets,
        )?);
    }

    let main_index = import_count + main_position as u32;
    let entry_index = import_count + module.functions.len() as u32 + runtime_count;

    let mut runtime_functions = Vec::new();
    if let (Some(type_index), Some(fd_write)) = (log_type, fd_write_index) {
        runtime_functions.push(RuntimeFunction {
            name: "ps_rt_log".into(),
            type_index,
            parameters: vec![ValType::I32],
            locals: Vec::new(),
            body: log_body(fd_write),
        });
    }

    let wasm = Module {
        name: module.name.clone(),
        imports,
        types,
        type_defs: Vec::new(),
        functions,
        runtime_functions,
        memories: vec![Memory {
            minimum: 1,
            maximum: None,
        }],
        data,
        exports: vec![
            Export {
                name: "main".into(),
                kind: ExportKind::Function,
                index: main_index,
            },
            Export {
                name: "_start".into(),
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
                Op::Leaf(Instruction::Call(0)),
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

struct Structurer<'a> {
    function: &'a MirFunction,
    blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    locals: HashMap<ValueId, u32>,
    function_indices: &'a HashMap<SymbolId, u32>,
    string_offsets: &'a HashMap<String, u32>,
}

impl Structurer<'_> {
    fn emit_region(
        &self,
        mut current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<Option<ValueId>, Vec<BackendError>> {
        loop {
            if stop == Some(current) {
                let block = self.blocks.get(&current).ok_or_else(|| {
                    wasm_error(self.function.span, "MIR branch merge block is missing")
                })?;
                return Ok(block.parameters.first().copied());
            }
            if !visited.insert(current) {
                return Err(wasm_error(
                    self.function.span,
                    "MIR contains a loop that the first Wasm structurer cannot lower",
                ));
            }
            let block = self
                .blocks
                .get(&current)
                .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
            self.emit_block_instructions(&block.instructions, body)?;
            let Some(terminator) = &block.terminator else {
                return Err(wasm_error(
                    self.function.span,
                    "MIR block has no terminator",
                ));
            };
            match terminator {
                Terminator::Return { .. } => return Ok(None),
                Terminator::Jump {
                    target,
                    arguments,
                    span,
                } => {
                    if stop == Some(*target) {
                        if arguments.len() != 1 {
                            return Err(wasm_error(
                                self.function.span,
                                "structured branch must pass one result value to its merge",
                            ));
                        }
                        return Ok(arguments.first().copied());
                    }
                    let target_block = self.blocks.get(target).ok_or_else(|| {
                        wasm_error(self.function.span, "MIR jump target is missing")
                    })?;
                    if target_block.parameters.len() != arguments.len() {
                        return Err(wasm_error(
                            self.function.span,
                            "MIR jump argument count differs from block parameters",
                        ));
                    }
                    for (argument, parameter) in
                        arguments.iter().zip(&target_block.parameters).rev()
                    {
                        body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            *argument,
                            *span,
                        )?)));
                        body.push(Op::Leaf(Instruction::LocalSet(local(
                            &self.locals,
                            *parameter,
                            *span,
                        )?)));
                    }
                    current = *target;
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                    merge_block,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *condition,
                        *span,
                    )?)));
                    let mut then_body = Body::new();
                    let then_value = self
                        .emit_region(*then_block, Some(*merge_block), visited, &mut then_body)?
                        .ok_or_else(|| {
                            wasm_error(self.function.span, "then arm does not reach its merge")
                        })?;
                    then_body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        then_value,
                        *span,
                    )?)));
                    let mut else_body = Body::new();
                    let else_value = self
                        .emit_region(*else_block, Some(*merge_block), visited, &mut else_body)?
                        .ok_or_else(|| {
                            wasm_error(self.function.span, "else arm does not reach its merge")
                        })?;
                    else_body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        else_value,
                        *span,
                    )?)));
                    body.push(Op::If {
                        then_body,
                        else_body,
                        result: Some(ValType::I32),
                        span: *span,
                    });
                    let merge = self
                        .blocks
                        .get(merge_block)
                        .ok_or_else(|| wasm_error(*span, "MIR merge block is missing"))?;
                    if merge.parameters.len() != 1 {
                        return Err(wasm_error(
                            *span,
                            "MIR branch merge must have one result value",
                        ));
                    }
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        merge.parameters[0],
                        *span,
                    )?)));
                    current = *merge_block;
                }
            }
        }
    }

    fn emit_block_instructions(
        &self,
        instructions: &[MirInstruction],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        for instruction in instructions {
            match instruction {
                MirInstruction::Constant {
                    destination,
                    value,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::I32Const(*value)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::StringConstant {
                    destination,
                    bytes,
                    span,
                } => {
                    let offset =
                        self.string_offsets.get(bytes).copied().ok_or_else(|| {
                            wasm_error(*span, "string constant has no data segment")
                        })?;
                    body.push(Op::Leaf(Instruction::I32Const(offset as i32)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::Copy {
                    destination,
                    value,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *value,
                        *span,
                    )?)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::Primitive {
                    destination,
                    op,
                    left,
                    right,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *left,
                        *span,
                    )?)));
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *right,
                        *span,
                    )?)));
                    body.push(Op::Leaf(primitive(*op)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::Call {
                    destination,
                    function,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            *argument,
                            *span,
                        )?)));
                    }
                    let index = self
                        .function_indices
                        .get(function)
                        .copied()
                        .ok_or_else(|| {
                            wasm_error(*span, "MIR call target has no Wasm function index")
                        })?;
                    body.push(Op::Leaf(Instruction::Call(index)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
            }
        }
        Ok(())
    }
}

fn primitive(op: Primitive) -> Instruction<'static> {
    match op {
        Primitive::Add => Instruction::I32Add,
        Primitive::Sub => Instruction::I32Sub,
        Primitive::Mul => Instruction::I32Mul,
        Primitive::DivS => Instruction::I32DivS,
        Primitive::RemS => Instruction::I32RemS,
        Primitive::Eq => Instruction::I32Eq,
        Primitive::Ne => Instruction::I32Ne,
        Primitive::LtS => Instruction::I32LtS,
        Primitive::LeS => Instruction::I32LeS,
        Primitive::GtS => Instruction::I32GtS,
        Primitive::GeS => Instruction::I32GeS,
    }
}

fn console_log_symbol(module: &mir::Module) -> Option<SymbolId> {
    module.externals.iter().find_map(|external| {
        (external.kind == ExternalKind::Runtime(HirRuntimeFunction::ConsoleLog))
            .then_some(external.symbol)
    })
}

fn collect_strings(module: &mir::Module) -> (HashMap<String, u32>, Vec<DataSegment>) {
    let mut offsets = HashMap::new();
    let mut data = Vec::new();
    let mut next = SCRATCH_END;
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let MirInstruction::StringConstant { bytes, .. } = instruction
                    && !offsets.contains_key(bytes)
                {
                    let offset = next.next_multiple_of(4);
                    offsets.insert(bytes.clone(), offset);
                    let mut segment = (bytes.len() as u32).to_le_bytes().to_vec();
                    segment.extend_from_slice(bytes.as_bytes());
                    data.push(DataSegment {
                        offset,
                        bytes: segment,
                    });
                    next = offset + 4 + bytes.len() as u32;
                }
            }
        }
    }
    (offsets, data)
}

/// Writes the string pointer's bytes followed by a newline, then returns unit
/// (zero). The runtime ABI tracks PureScript's `console.log`, which terminates
/// each write with a newline. Each `fd_write` uses a single iovec because a
/// host is allowed to complete only a partial write of a multi-entry vector.
fn log_body(fd_write_index: u32) -> Body {
    let mem = MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    };
    let write = |body: &mut Body, cursor: Vec<Op>| {
        body.extend(cursor);
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(NWRITTEN_ADDR as i32)));
        body.push(Op::Leaf(Instruction::Call(fd_write_index)));
        body.push(Op::Leaf(Instruction::Drop));
    };
    let mut body = Body::new();
    // iovec = { buffer: pointer + 4, length: *pointer }
    write(
        &mut body,
        vec![
            Op::Leaf(Instruction::I32Const(0)),
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::I32Const(4)),
            Op::Leaf(Instruction::I32Add),
            Op::Leaf(Instruction::I32Store(mem)),
            Op::Leaf(Instruction::I32Const(4)),
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::I32Load(mem)),
            Op::Leaf(Instruction::I32Store(mem)),
        ],
    );
    // iovec = { buffer: newline, length: 1 }
    write(
        &mut body,
        vec![
            Op::Leaf(Instruction::I32Const(0)),
            Op::Leaf(Instruction::I32Const(NEWLINE_ADDR as i32)),
            Op::Leaf(Instruction::I32Store(mem)),
            Op::Leaf(Instruction::I32Const(4)),
            Op::Leaf(Instruction::I32Const(1)),
            Op::Leaf(Instruction::I32Store(mem)),
        ],
    );
    body.push(Op::Leaf(Instruction::I32Const(0)));
    body
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

fn local(
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

fn wasm_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P10 Wasm structuring", span, message)]
}
