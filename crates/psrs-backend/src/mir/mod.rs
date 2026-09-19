use crate::abi::{self, WasiRegistry, names};
use crate::types::{RecGroup, ValueDecl, ValueId, ValueType};
use crate::{
    BackendError,
    cc::{self, AssignmentKind},
};
use psrs_core::Primitive;
use psrs_hir::{ExternalSymbol, SymbolId};
use psrs_span::TextRange;

mod instruction;
mod verify;

pub use instruction::Instruction;
pub use verify::verify_module;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    /// Defined GC types owned by MIR. The Wasm encoding emits them after the
    /// function types at a fixed base; see `docs/design/D-06`.
    pub types: Vec<RecGroup>,
    /// Runtime ABI imports the module may call. Their canonical signatures come
    /// from the WIT runtime ABI; see `docs/decision/DEC-06`.
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    pub span: TextRange,
}

/// A runtime ABI import, lowered to its canonical ABI signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub symbol: SymbolId,
    pub module: String,
    pub name: String,
    pub parameters: Vec<ValueType>,
    pub result: Option<ValueType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub parameters: Vec<ValueId>,
    pub values: Vec<ValueDecl>,
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
    pub result: ValueId,
    pub result_type: ValueType,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub parameters: Vec<ValueId>,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Return {
        value: ValueId,
        span: TextRange,
    },
    Jump {
        target: BlockId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
        merge_block: BlockId,
        span: TextRange,
    },
}

pub fn lower_module(module: cc::Module) -> Result<Module, Vec<BackendError>> {
    let mut wasi = WasiRegistry::load()
        .map_err(|message| vec![BackendError::new("P9 MIR lowering", module.span, message)])?;
    let mut functions = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        functions.push(lower_function(function, &mut wasi)?);
    }
    let imports = wasi
        .imports()
        .iter()
        .map(|import| Import {
            symbol: import.symbol,
            module: import.module.clone(),
            name: import.name.clone(),
            parameters: import.parameters.clone(),
            result: import.result,
        })
        .collect();
    let mir = Module {
        name: module.name,
        externals: module.externals,
        types: Vec::new(),
        imports,
        functions,
        span: module.span,
    };
    verify_module(&mir)?;
    Ok(mir)
}

fn lower_function(
    source: &cc::Function,
    wasi: &mut WasiRegistry,
) -> Result<Function, Vec<BackendError>> {
    let entry = BlockId(0);
    let mut lowerer = FunctionLowerer {
        next_block: 1,
        blocks: vec![BasicBlock {
            id: entry,
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        }],
        values: source.values.clone(),
        next_value: source
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .map_or(0, |max| max + 1),
        wasi,
    };
    let end = lowerer.lower_assignments(&source.assignments, entry)?;
    lowerer.set_terminator(
        end,
        Terminator::Return {
            value: source.result,
            span: source.span,
        },
        source.span,
    )?;
    Ok(Function {
        symbol: source.symbol,
        name: source.name.clone(),
        parameters: source.parameters.clone(),
        values: lowerer.values,
        entry,
        blocks: lowerer.blocks,
        result: source.result,
        result_type: source.result_type,
        span: source.span,
    })
}

struct FunctionLowerer<'a> {
    next_block: u32,
    blocks: Vec<BasicBlock>,
    values: Vec<ValueDecl>,
    next_value: u32,
    wasi: &'a mut WasiRegistry,
}

impl FunctionLowerer<'_> {
    fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
    }

    /// Lowers `log` to WASI: get standard output, then write the string and a
    /// newline through `blocking-write-and-flush`.
    fn lower_print(
        &mut self,
        destination: ValueId,
        argument: ValueId,
        span: TextRange,
        current: BlockId,
    ) -> Result<(), Vec<BackendError>> {
        let get_stdout = self
            .wasi
            .import(names::STDOUT, names::GET_STDOUT)
            .map_err(|message| vec![BackendError::new("P9 MIR lowering", span, message)])?;
        let write = self
            .wasi
            .import(names::STREAMS, names::WRITE_STDOUT)
            .map_err(|message| vec![BackendError::new("P9 MIR lowering", span, message)])?;

        let handle = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Call {
                destination: handle,
                function: get_stdout.symbol,
                arguments: Vec::new(),
                span,
            },
            span,
        )?;

        let length = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Load {
                destination: length,
                address: argument,
                offset: 0,
                span,
            },
            span,
        )?;
        let four = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Constant {
                destination: four,
                value: 4,
                span,
            },
            span,
        )?;
        let bytes = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Primitive {
                destination: bytes,
                op: Primitive::Add,
                left: argument,
                right: four,
                span,
            },
            span,
        )?;
        let scratch = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Constant {
                destination: scratch,
                value: abi::PRINT_SCRATCH,
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::CallVoid {
                function: write.symbol,
                arguments: vec![handle, bytes, length, scratch],
                span,
            },
            span,
        )?;

        let newline = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Constant {
                destination: newline,
                value: abi::NEWLINE_ADDR as i32,
                span,
            },
            span,
        )?;
        let one = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Constant {
                destination: one,
                value: 1,
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::CallVoid {
                function: write.symbol,
                arguments: vec![handle, newline, one, scratch],
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::Constant {
                destination,
                value: 0,
                span,
            },
            span,
        )?;
        Ok(())
    }

    fn lower_assignments(
        &mut self,
        assignments: &[cc::Assignment],
        mut current: BlockId,
    ) -> Result<BlockId, Vec<BackendError>> {
        for assignment in assignments {
            match &assignment.kind {
                AssignmentKind::Constant(value) => self.append_instruction(
                    current,
                    Instruction::Constant {
                        destination: assignment.destination,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::StringConstant(bytes) => self.append_instruction(
                    current,
                    Instruction::StringConstant {
                        destination: assignment.destination,
                        bytes: bytes.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::Copy(value) => self.append_instruction(
                    current,
                    Instruction::Copy {
                        destination: assignment.destination,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::Primitive { op, left, right } => self.append_instruction(
                    current,
                    Instruction::Primitive {
                        destination: assignment.destination,
                        op: *op,
                        left: *left,
                        right: *right,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::DirectCall {
                    function,
                    arguments,
                } => {
                    if is_print(function) {
                        let argument = arguments.first().copied().ok_or_else(|| {
                            vec![BackendError::new(
                                "P9 MIR lowering",
                                assignment.span,
                                "`log` takes one argument",
                            )]
                        })?;
                        self.lower_print(
                            assignment.destination,
                            argument,
                            assignment.span,
                            current,
                        )?;
                    } else {
                        self.append_instruction(
                            current,
                            Instruction::Call {
                                destination: assignment.destination,
                                function: *function,
                                arguments: arguments.clone(),
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    }
                }
                AssignmentKind::If {
                    condition,
                    then_assignments,
                    then_value,
                    else_assignments,
                    else_value,
                } => {
                    let then_block = self.new_block(Vec::new());
                    let else_block = self.new_block(Vec::new());
                    let merge = self.new_block(vec![assignment.destination]);
                    self.set_terminator(
                        current,
                        Terminator::Branch {
                            condition: *condition,
                            then_block,
                            else_block,
                            merge_block: merge,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    let then_end = self.lower_assignments(then_assignments, then_block)?;
                    self.set_terminator(
                        then_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*then_value],
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    let else_end = self.lower_assignments(else_assignments, else_block)?;
                    self.set_terminator(
                        else_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*else_value],
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    current = merge;
                }
            }
        }
        Ok(current)
    }

    fn new_block(&mut self, parameters: Vec<ValueId>) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(BasicBlock {
            id,
            parameters,
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    fn append_instruction(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self.find_block_mut(block, span)?;
        if target.terminator.is_some() {
            return Err(vec![BackendError::new(
                "P9 MIR lowering",
                span,
                "cannot append an instruction after a terminator",
            )]);
        }
        target.instructions.push(instruction);
        Ok(())
    }

    fn set_terminator(
        &mut self,
        block: BlockId,
        terminator: Terminator,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self.find_block_mut(block, span)?;
        if target.terminator.replace(terminator).is_some() {
            return Err(vec![BackendError::new(
                "P9 MIR lowering",
                span,
                "basic block already has a terminator",
            )]);
        }
        Ok(())
    }

    fn find_block_mut(
        &mut self,
        id: BlockId,
        span: TextRange,
    ) -> Result<&mut BasicBlock, Vec<BackendError>> {
        self.blocks
            .iter_mut()
            .find(|block| block.id == id)
            .ok_or_else(|| {
                vec![BackendError::new(
                    "P9 MIR lowering",
                    span,
                    "basic block ID was not allocated",
                )]
            })
    }
}

/// Whether the callee is the standard library's `print` host function.
fn is_print(symbol: &SymbolId) -> bool {
    psrs_hir::host_function("log").is_some_and(|function| function.symbol == *symbol)
}
