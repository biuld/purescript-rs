use crate::BackendError;
use crate::cc::{ValueId, ValueType};
use crate::mir::{
    self, BlockId, Function as MirFunction, Instruction as MirInstruction, Terminator,
};
use psrs_core::Primitive;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, InstructionSink,
    Module, TypeSection, ValType,
};

pub(super) fn lower_module(module: &mir::Module) -> Result<Vec<u8>, Vec<BackendError>> {
    mir::verify_module(module)?;
    let Some(main_index) = module.functions.iter().position(|function| {
        function.name == "main"
            && function.parameters.is_empty()
            && function.result_type == ValueType::I32
    }) else {
        return Err(wasm_error(
            module.span,
            "the first Wasm artifact requires a zero-argument Int `main` declaration",
        ));
    };

    let function_indices = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.symbol, index as u32))
        .collect::<HashMap<_, _>>();
    let (type_arities, function_types) = collect_function_types(module)?;
    let mut encoder = Module::new();

    if !type_arities.is_empty() {
        let mut types = TypeSection::new();
        for arity in &type_arities {
            types
                .ty()
                .function((0..*arity).map(|_| ValType::I32), [ValType::I32]);
        }
        encoder.section(&types);
    }

    if !module.functions.is_empty() {
        let mut functions = FunctionSection::new();
        for type_index in &function_types {
            functions.function(*type_index);
        }
        encoder.section(&functions);
    }

    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, main_index as u32);
    encoder.section(&exports);

    let mut code = CodeSection::new();
    for source in &module.functions {
        let locals = local_indices(source)?;
        let local_count = source.values.len() - source.parameters.len();
        let groups = if local_count == 0 {
            Vec::new()
        } else {
            vec![(local_count as u32, ValType::I32)]
        };
        let mut function = Function::new(groups);
        {
            let mut instructions = function.instructions();
            let structurer = Structurer {
                function: source,
                blocks: source
                    .blocks
                    .iter()
                    .map(|block| (block.id, block))
                    .collect(),
                locals,
                function_indices: &function_indices,
            };
            structurer.emit_region(source.entry, None, &mut HashSet::new(), &mut instructions)?;
            let result = local(&structurer.locals, source.result, source.span)?;
            instructions.local_get(result);
            instructions.end();
        }
        code.function(&function);
    }
    encoder.section(&code);

    Ok(encoder.finish())
}

fn collect_function_types(
    module: &mir::Module,
) -> Result<(Vec<usize>, Vec<u32>), Vec<BackendError>> {
    let mut arities = Vec::<usize>::new();
    let mut indices = HashMap::<usize, u32>::new();
    let mut function_types = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        if function.parameters.len() > function.values.len() {
            return Err(wasm_error(
                function.span,
                "MIR function has more parameters than values",
            ));
        }
        for parameter in &function.parameters {
            if value_type(function, *parameter).is_none() {
                return Err(wasm_error(
                    function.span,
                    "MIR function parameter has no value type",
                ));
            }
        }
        let arity = function.parameters.len();
        let next = arities.len() as u32;
        let type_index = *indices.entry(arity).or_insert_with(|| {
            arities.push(arity);
            next
        });
        function_types.push(type_index);
    }
    Ok((arities, function_types))
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

struct Structurer<'a> {
    function: &'a MirFunction,
    blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    locals: HashMap<ValueId, u32>,
    function_indices: &'a HashMap<SymbolId, u32>,
}

impl Structurer<'_> {
    fn emit_region(
        &self,
        mut current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        output: &mut InstructionSink<'_>,
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
            self.emit_block_instructions(&block.instructions, output)?;
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
                        output.local_get(local(&self.locals, *argument, *span)?);
                        output.local_set(local(&self.locals, *parameter, *span)?);
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
                    output.local_get(local(&self.locals, *condition, *span)?);
                    output.if_(BlockType::Result(ValType::I32));
                    let then_value = self
                        .emit_region(*then_block, Some(*merge_block), visited, output)?
                        .ok_or_else(|| {
                            wasm_error(self.function.span, "then arm does not reach its merge")
                        })?;
                    output.local_get(local(&self.locals, then_value, *span)?);
                    output.else_();
                    let else_value = self
                        .emit_region(*else_block, Some(*merge_block), visited, output)?
                        .ok_or_else(|| {
                            wasm_error(self.function.span, "else arm does not reach its merge")
                        })?;
                    output.local_get(local(&self.locals, else_value, *span)?);
                    output.end();
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
                    output.local_set(local(&self.locals, merge.parameters[0], *span)?);
                    current = *merge_block;
                }
            }
        }
    }

    fn emit_block_instructions(
        &self,
        instructions: &[MirInstruction],
        output: &mut InstructionSink<'_>,
    ) -> Result<(), Vec<BackendError>> {
        for instruction in instructions {
            match instruction {
                MirInstruction::Constant {
                    destination,
                    value,
                    span,
                } => {
                    output.i32_const(*value);
                    output.local_set(local(&self.locals, *destination, *span)?);
                }
                MirInstruction::Copy {
                    destination,
                    value,
                    span,
                } => {
                    output.local_get(local(&self.locals, *value, *span)?);
                    output.local_set(local(&self.locals, *destination, *span)?);
                }
                MirInstruction::Primitive {
                    destination,
                    op,
                    left,
                    right,
                    span,
                } => {
                    output.local_get(local(&self.locals, *left, *span)?);
                    output.local_get(local(&self.locals, *right, *span)?);
                    emit_primitive(*op, output);
                    output.local_set(local(&self.locals, *destination, *span)?);
                }
                MirInstruction::Call {
                    destination,
                    function,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        output.local_get(local(&self.locals, *argument, *span)?);
                    }
                    let index = self
                        .function_indices
                        .get(function)
                        .copied()
                        .ok_or_else(|| {
                            wasm_error(*span, "MIR call target has no Wasm function index")
                        })?;
                    output.call(index);
                    output.local_set(local(&self.locals, *destination, *span)?);
                }
            }
        }
        Ok(())
    }
}

fn emit_primitive(op: Primitive, output: &mut InstructionSink<'_>) {
    match op {
        Primitive::Add => output.i32_add(),
        Primitive::Sub => output.i32_sub(),
        Primitive::Mul => output.i32_mul(),
        Primitive::DivS => output.i32_div_s(),
        Primitive::RemS => output.i32_rem_s(),
        Primitive::Eq => output.i32_eq(),
        Primitive::Ne => output.i32_ne(),
        Primitive::LtS => output.i32_lt_s(),
        Primitive::LeS => output.i32_le_s(),
        Primitive::GtS => output.i32_gt_s(),
        Primitive::GeS => output.i32_ge_s(),
    };
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
    vec![BackendError::new("P10 Wasm emission", span, message)]
}
