use crate::BackendError;
use crate::cc::{self, AssignmentKind};
use crate::types::{RecGroup, ValueDecl, ValueId, ValueType};
use psrs_core::Primitive;
use psrs_hir::{ExternalSymbol, SymbolId};
use psrs_span::TextRange;

mod verify;
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
    pub functions: Vec<Function>,
    pub span: TextRange,
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
pub enum Instruction {
    Constant {
        destination: ValueId,
        value: i32,
        span: TextRange,
    },
    StringConstant {
        destination: ValueId,
        bytes: String,
        span: TextRange,
    },
    Copy {
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    },
    Primitive {
        destination: ValueId,
        op: Primitive,
        left: ValueId,
        right: ValueId,
        span: TextRange,
    },
    Call {
        destination: ValueId,
        function: SymbolId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
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
    let mut functions = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        functions.push(lower_function(function)?);
    }
    let mir = Module {
        name: module.name,
        externals: module.externals,
        types: Vec::new(),
        functions,
        span: module.span,
    };
    verify_module(&mir)?;
    Ok(mir)
}

fn lower_function(source: &cc::Function) -> Result<Function, Vec<BackendError>> {
    let entry = BlockId(0);
    let mut lowerer = FunctionLowerer {
        next_block: 1,
        blocks: vec![BasicBlock {
            id: entry,
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        }],
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
        values: source.values.clone(),
        entry,
        blocks: lowerer.blocks,
        result: source.result,
        result_type: source.result_type,
        span: source.span,
    })
}

struct FunctionLowerer {
    next_block: u32,
    blocks: Vec<BasicBlock>,
}

impl FunctionLowerer {
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
                } => self.append_instruction(
                    current,
                    Instruction::Call {
                        destination: assignment.destination,
                        function: *function,
                        arguments: arguments.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
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
