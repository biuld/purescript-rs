//! P9 lowering for the non-GC linear-memory representation.

use super::layout::LayoutError;
use super::planner::LinearMemoryLayout;
use super::{BasicBlock, BlockId, Function, Instruction, Terminator};
use crate::BackendError;
use crate::cc::{self, AssignmentKind, RefShape, ValueShape};
use crate::types::{FunctionId, TableSlot, ValueDecl, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;

mod arrays;
mod helpers;

fn layout_error(span: TextRange, error: LayoutError) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        format!("invalid linear-memory layout request: {error:?}"),
    )]
}

fn unsupported(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR lowering", span, message)]
}

pub(super) fn lower_function(
    source: &cc::Function,
    id: FunctionId,
    layout: &LinearMemoryLayout,
    wit_imports: &HashMap<SymbolId, crate::abi::WasiImport>,
    table_slots: &HashMap<SymbolId, TableSlot>,
) -> Result<Function, Vec<BackendError>> {
    let entry = BlockId(0);
    let mut lowerer = LinearFunctionLowerer {
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
            .map(|value| ValueDecl {
                id: value.id,
                ty: LinearMemoryLayout::value_type(value.ty),
            })
            .collect(),
        shapes: source
            .values
            .iter()
            .map(|value| (value.id, value.ty))
            .collect(),
        next_value: source
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .map_or(0, |max| max + 1),
        layout,
        wit_imports,
        table_slots,
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
        result_type: LinearMemoryLayout::value_type(source.result_type),
        span: source.span,
    })
}

pub(super) struct LinearFunctionLowerer<'a> {
    pub(super) next_block: u32,
    pub(super) blocks: Vec<BasicBlock>,
    pub(super) values: Vec<ValueDecl>,
    pub(super) shapes: HashMap<ValueId, ValueShape>,
    pub(super) next_value: u32,
    pub(super) layout: &'a LinearMemoryLayout,
    pub(super) wit_imports: &'a HashMap<SymbolId, crate::abi::WasiImport>,
    pub(super) table_slots: &'a HashMap<SymbolId, TableSlot>,
}

impl LinearFunctionLowerer<'_> {
    pub(super) fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
    }

    fn shape(&self, value: ValueId, span: TextRange) -> Result<ValueShape, Vec<BackendError>> {
        self.shapes
            .get(&value)
            .copied()
            .ok_or_else(|| unsupported(span, "linear-memory value has no abstract shape"))
    }

    fn lower_assignments(
        &mut self,
        assignments: &[cc::Assignment],
        mut current: BlockId,
    ) -> Result<BlockId, Vec<BackendError>> {
        for assignment in assignments {
            let span = assignment.span;
            match &assignment.kind {
                AssignmentKind::Constant(value) => self.append(
                    current,
                    Instruction::Constant {
                        destination: assignment.destination,
                        value: *value,
                        span,
                    },
                    span,
                )?,
                AssignmentKind::NumberConstant(value) => self.append(
                    current,
                    Instruction::NumberConstant {
                        destination: assignment.destination,
                        value: value.clone(),
                        span,
                    },
                    span,
                )?,
                AssignmentKind::StringConstant(bytes) => self.append(
                    current,
                    Instruction::StringConstant {
                        destination: assignment.destination,
                        bytes: bytes.clone(),
                        span,
                    },
                    span,
                )?,
                AssignmentKind::Primitive { op, left, right } => self.append(
                    current,
                    Instruction::Primitive {
                        destination: assignment.destination,
                        op: *op,
                        left: *left,
                        right: *right,
                        span,
                    },
                    span,
                )?,
                AssignmentKind::ProductNew {
                    destination,
                    representation,
                    arguments,
                } => {
                    let bytes = self
                        .layout
                        .representation_size(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearAlloc {
                            destination: *destination,
                            bytes,
                            span,
                        },
                        span,
                    )?;
                    for (field, argument) in arguments.iter().enumerate() {
                        let (offset, shape) = self
                            .layout
                            .field(*representation, field as u32)
                            .map_err(|error| layout_error(span, error))?;
                        self.append(
                            current,
                            Instruction::LinearStore {
                                address: *destination,
                                value: *argument,
                                offset,
                                ty: LinearMemoryLayout::value_type(shape),
                                span,
                            },
                            span,
                        )?;
                    }
                }
                AssignmentKind::ProductGet {
                    destination,
                    representation,
                    field,
                    value,
                } => {
                    let (offset, shape) = self
                        .layout
                        .field(*representation, *field)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearLoad {
                            destination: *destination,
                            address: *value,
                            offset,
                            ty: LinearMemoryLayout::value_type(shape),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ArrayNew {
                    destination,
                    representation,
                    elements,
                } => {
                    let (element, stride) = self
                        .layout
                        .array(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    let bytes = 4u32
                        .checked_add(stride.saturating_mul(elements.len() as u32))
                        .ok_or_else(|| {
                            unsupported(span, "linear-memory array allocation is too large")
                        })?;
                    self.append(
                        current,
                        Instruction::LinearAlloc {
                            destination: *destination,
                            bytes,
                            span,
                        },
                        span,
                    )?;
                    let length = self.fresh(ValueType::I32);
                    self.append(
                        current,
                        Instruction::Constant {
                            destination: length,
                            value: elements.len() as i32,
                            span,
                        },
                        span,
                    )?;
                    self.append(
                        current,
                        Instruction::LinearStore {
                            address: *destination,
                            value: length,
                            offset: 0,
                            ty: ValueType::I32,
                            span,
                        },
                        span,
                    )?;
                    for (index, element_value) in elements.iter().enumerate() {
                        self.append(
                            current,
                            Instruction::LinearStore {
                                address: *destination,
                                value: *element_value,
                                offset: 4 + stride * index as u32,
                                ty: LinearMemoryLayout::value_type(element),
                                span,
                            },
                            span,
                        )?;
                    }
                }
                AssignmentKind::ArrayLen { destination, value } => {
                    let shape = self.shape(*value, span)?;
                    let ValueShape::Reference(reference) = shape else {
                        return Err(unsupported(
                            span,
                            "linear-memory array.len requires an array pointer",
                        ));
                    };
                    if !matches!(reference.heap, RefShape::Repr(_)) {
                        return Err(unsupported(
                            span,
                            "linear-memory array.len requires an array representation",
                        ));
                    }
                    self.append(
                        current,
                        Instruction::LinearLoad {
                            destination: *destination,
                            address: *value,
                            offset: 0,
                            ty: ValueType::I32,
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ArrayGet {
                    destination,
                    representation,
                    value,
                    index,
                } => {
                    let (element, stride) = self
                        .layout
                        .array(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    let address = self.index_address(current, *value, *index, stride, span)?;
                    self.append(
                        current,
                        Instruction::LinearLoad {
                            destination: *destination,
                            address,
                            offset: 4,
                            ty: LinearMemoryLayout::value_type(element),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ArrayClone {
                    destination,
                    representation,
                    value,
                } => {
                    self.lower_array_clone(current, *destination, *representation, *value, span)?
                }
                AssignmentKind::ArraySet {
                    representation,
                    value,
                    index,
                    new_value,
                    ..
                } => {
                    let (element, stride) = self
                        .layout
                        .array(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    let address = self.index_address(current, *value, *index, stride, span)?;
                    self.append(
                        current,
                        Instruction::LinearStore {
                            address,
                            value: *new_value,
                            offset: 4,
                            ty: LinearMemoryLayout::value_type(element),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::DirectCall {
                    function,
                    arguments,
                } => {
                    if self.wit_imports.contains_key(function) {
                        return Err(unsupported(
                            span,
                            "linear-memory target does not support canonical WIT calls yet",
                        ));
                    }
                    self.append(
                        current,
                        Instruction::Call {
                            destination: assignment.destination,
                            function: *function,
                            arguments: arguments.clone(),
                            span,
                        },
                        span,
                    )?;
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
                            span,
                        },
                        span,
                    )?;
                    let then_end = self.lower_assignments(then_assignments, then_block)?;
                    self.set_terminator(
                        then_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*then_value],
                            span,
                        },
                        span,
                    )?;
                    let else_end = self.lower_assignments(else_assignments, else_block)?;
                    self.set_terminator(
                        else_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*else_value],
                            span,
                        },
                        span,
                    )?;
                    current = merge;
                }
                AssignmentKind::FunctionRef {
                    function,
                    signature,
                    captures,
                } => {
                    let table_slot = self.table_slots.get(function).copied().ok_or_else(|| {
                        unsupported(span, "linear closure target has no function-table slot")
                    })?;
                    let type_index = self
                        .layout
                        .signature_index(*signature)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearClosureNew {
                            destination: assignment.destination,
                            function: *function,
                            table_slot,
                            type_index,
                            captures: captures.clone(),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::IndirectCall {
                    function,
                    signature,
                    arguments,
                } => {
                    let type_index = self
                        .layout
                        .signature_index(*signature)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearClosureCall {
                            destination: assignment.destination,
                            function: *function,
                            type_index,
                            arguments: arguments.clone(),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ClosureGetCapture { closure, index } => {
                    let shape = self.shape(assignment.destination, span)?;
                    self.append(
                        current,
                        Instruction::LinearClosureGetCapture {
                            destination: assignment.destination,
                            closure: *closure,
                            index: *index,
                            ty: LinearMemoryLayout::value_type(shape),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::RepresentationTest { .. }
                | AssignmentKind::RepresentationCast { .. } => {
                    return Err(unsupported(
                        span,
                        "linear-memory target does not support dynamic representation tests yet",
                    ));
                }
            }
        }
        Ok(current)
    }
}
