use super::layout::{LayoutError, PlannedLayout};
use super::wit;
use super::{BasicBlock, BlockId, Function, Terminator};
use crate::BackendError;
use crate::abi::WasiImport;
use crate::cc::{self, AssignmentKind};
use crate::mir::instruction::Instruction;
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;

fn layout_error(span: TextRange, error: LayoutError) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        format!("invalid concrete layout request: {error:?}"),
    )]
}

pub(super) fn lower_function(
    source: &cc::Function,
    id: FunctionId,
    wit_imports: &HashMap<SymbolId, WasiImport>,
    layout: &PlannedLayout,
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
        values: source
            .values
            .iter()
            .map(|value| {
                Ok(ValueDecl {
                    id: value.id,
                    ty: layout
                        .value_type(&value.ty)
                        .map_err(|error| layout_error(source.span, error))?,
                })
            })
            .collect::<Result<Vec<_>, Vec<BackendError>>>()?,
        next_value: source
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .map_or(0, |max| max + 1),
        wit_imports,
        layout,
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
        id,
        symbol: source.symbol,
        name: source.name.clone(),
        parameters: source.parameters.clone(),
        values: lowerer.values,
        entry,
        blocks: lowerer.blocks,
        result: source.result,
        result_type: layout
            .value_type(&source.result_type)
            .map_err(|error| layout_error(source.span, error))?,
        span: source.span,
    })
}
pub(super) struct FunctionLowerer<'a> {
    next_block: u32,
    blocks: Vec<BasicBlock>,
    values: Vec<ValueDecl>,
    next_value: u32,
    wit_imports: &'a HashMap<SymbolId, WasiImport>,
    layout: &'a PlannedLayout,
}

impl FunctionLowerer<'_> {
    pub(super) fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
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
                AssignmentKind::NumberConstant(value) => self.append_instruction(
                    current,
                    Instruction::NumberConstant {
                        destination: assignment.destination,
                        value: value.clone(),
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
                AssignmentKind::RepresentationTest {
                    destination,
                    value,
                    reference,
                } => self.append_instruction(
                    current,
                    Instruction::RefTest {
                        destination: *destination,
                        value: *value,
                        reference: self
                            .layout
                            .reference(reference)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::RepresentationCast {
                    destination,
                    value,
                    reference,
                } => self.append_instruction(
                    current,
                    Instruction::RefCast {
                        destination: *destination,
                        value: *value,
                        reference: self
                            .layout
                            .reference(reference)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ProductNew {
                    destination,
                    representation,
                    arguments,
                } => self.append_instruction(
                    current,
                    Instruction::StructNew {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        arguments: arguments.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ProductGet {
                    destination,
                    representation,
                    field,
                    value,
                } => self.append_instruction(
                    current,
                    Instruction::StructGet {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        field: *field,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArrayNew {
                    destination,
                    representation,
                    elements,
                } => self.append_instruction(
                    current,
                    Instruction::ArrayNew {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        elements: elements.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArrayLen { destination, value } => self.append_instruction(
                    current,
                    Instruction::ArrayLen {
                        destination: *destination,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArrayGet {
                    destination,
                    representation,
                    value,
                    index,
                } => {
                    let type_index = self
                        .layout
                        .repr_index(*representation)
                        .map_err(|error| layout_error(assignment.span, error))?;
                    let destination_type = self
                        .values
                        .iter()
                        .find(|candidate| candidate.id == *destination)
                        .map(|candidate| candidate.ty)
                        .ok_or_else(|| {
                            vec![BackendError::new(
                                "P9 MIR lowering",
                                assignment.span,
                                "array.get destination has no value declaration",
                            )]
                        })?;
                    if let ValueType::Ref(reference) = destination_type
                        && !reference.nullable
                    {
                        let temporary = self.fresh(ValueType::Ref(crate::types::RefType {
                            nullable: true,
                            heap: reference.heap,
                        }));
                        self.append_instruction(
                            current,
                            Instruction::ArrayGet {
                                destination: temporary,
                                type_index,
                                value: *value,
                                index: *index,
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                        self.append_instruction(
                            current,
                            Instruction::RefCast {
                                destination: *destination,
                                value: temporary,
                                reference,
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    } else {
                        self.append_instruction(
                            current,
                            Instruction::ArrayGet {
                                destination: *destination,
                                type_index,
                                value: *value,
                                index: *index,
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    }
                }
                AssignmentKind::ArrayClone {
                    destination,
                    representation,
                    value,
                } => self.append_instruction(
                    current,
                    Instruction::ArrayClone {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArraySet {
                    representation,
                    value,
                    index,
                    new_value,
                    ..
                } => self.append_instruction(
                    current,
                    Instruction::ArraySet {
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        value: *value,
                        index: *index,
                        new_value: *new_value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::DirectCall {
                    function,
                    arguments,
                } => {
                    if let Some(import) = self.wit_imports.get(function).cloned() {
                        wit::lower(
                            self,
                            &import,
                            assignment.destination,
                            arguments,
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
                AssignmentKind::FunctionRef {
                    function,
                    signature,
                    captures,
                } => {
                    let (closure_type, capture_array_type) = self
                        .layout
                        .closure_layout()
                        .map_err(|error| layout_error(assignment.span, error))?;
                    self.append_instruction(
                        current,
                        Instruction::ClosureNew {
                            destination: assignment.destination,
                            function: *function,
                            type_index: self
                                .layout
                                .signature_index(*signature)
                                .map_err(|error| layout_error(assignment.span, error))?,
                            closure_type,
                            capture_array_type,
                            boxed_integer_type: self.layout.boxed_integer_index(),
                            boxed_f64_type: self.layout.boxed_number_index(),
                            captures: captures.clone(),
                            span: assignment.span,
                        },
                        assignment.span,
                    )?
                }
                AssignmentKind::IndirectCall {
                    function,
                    signature,
                    arguments,
                } => {
                    let (closure_type, capture_array_type) = self
                        .layout
                        .closure_layout()
                        .map_err(|error| layout_error(assignment.span, error))?;
                    self.append_instruction(
                        current,
                        Instruction::ClosureCall {
                            destination: assignment.destination,
                            function: *function,
                            type_index: self
                                .layout
                                .signature_index(*signature)
                                .map_err(|error| layout_error(assignment.span, error))?,
                            closure_type,
                            capture_array_type,
                            arguments: arguments.clone(),
                            span: assignment.span,
                        },
                        assignment.span,
                    )?
                }
                AssignmentKind::ClosureGetCapture { closure, index } => {
                    let (closure_type, capture_array_type) = self
                        .layout
                        .closure_layout()
                        .map_err(|error| layout_error(assignment.span, error))?;
                    self.append_instruction(
                        current,
                        Instruction::ClosureGetCapture {
                            destination: assignment.destination,
                            closure: *closure,
                            closure_type,
                            capture_array_type,
                            boxed_integer_type: self.layout.boxed_integer_index(),
                            boxed_f64_type: self.layout.boxed_number_index(),
                            index: *index,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?
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

    pub(super) fn append_instruction(
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
