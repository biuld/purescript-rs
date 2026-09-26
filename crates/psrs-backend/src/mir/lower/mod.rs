use super::layout::{LayoutError, PlannedLayout};
use super::literals::StringLiterals;
use super::wit;
use super::{BasicBlock, BlockId, Function, Terminator};
use crate::BackendError;
use crate::abi::BoundWasiImport;
use crate::cc::{self, AssignmentKind};
use crate::mir::instruction::Instruction;
use crate::mir::scalar_helpers::ScalarHelpers;
use crate::types::{FunctionId, HeapType, ValueDecl, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;

mod aggregate;
mod assignment_array;
mod assignments;
mod conversion_helpers;
mod tail;
mod variant;
pub(super) use conversion_helpers::ConversionHelpers;
#[cfg(test)]
mod wit_tests;

fn layout_error(span: TextRange, error: LayoutError) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        format!("invalid concrete layout request: {error:?}"),
    )]
}

fn unsupported_wit_record(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "WIT record argument has no concrete product layout",
    )]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_function(
    source: &cc::Function,
    id: FunctionId,
    wit_imports: &HashMap<SymbolId, BoundWasiImport>,
    scalar_helpers: &ScalarHelpers,
    layout: &PlannedLayout,
    conversion_helpers: Option<&mut ConversionHelpers>,
    literals: Option<&mut StringLiterals>,
    target: crate::capability::TargetCapabilities,
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
        scalar_helpers,
        layout,
        conversion_helpers,
        literals,
        owned_handles: Vec::new(),
    };
    let end = lowerer.lower_assignments(&source.assignments, entry)?;
    lowerer.discharge_owned_handles(end, source.result)?;
    lowerer.set_terminator(
        end,
        Terminator::Return {
            value: source.result,
            span: source.span,
        },
        source.span,
    )?;
    let mut function = Function {
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
    };
    tail::mark_tail(&mut function, target)?;
    wit::verify_function(&function, wit_imports)?;
    Ok(function)
}
pub(super) struct FunctionLowerer<'a> {
    next_block: u32,
    blocks: Vec<BasicBlock>,
    values: Vec<ValueDecl>,
    next_value: u32,
    wit_imports: &'a HashMap<SymbolId, BoundWasiImport>,
    scalar_helpers: &'a ScalarHelpers,
    layout: &'a PlannedLayout,
    conversion_helpers: Option<&'a mut ConversionHelpers>,
    literals: Option<&'a mut StringLiterals>,
    owned_handles: Vec<wit::OwnedObligation>,
}

impl FunctionLowerer<'_> {
    pub(super) fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
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
    pub(super) fn note_owned_handle(
        &mut self,
        value: ValueId,
        drop_symbol: SymbolId,
        span: TextRange,
    ) {
        self.owned_handles.push(wit::OwnedObligation {
            value,
            drop_symbol,
            span,
        });
    }

    pub(super) fn transfer_owned_handle(&mut self, value: ValueId) {
        self.owned_handles
            .retain(|obligation| obligation.value != value);
    }

    /// Inserts `resource.drop` for owned handles this function did not return
    /// and did not pass to an `own<T>` parameter.
    fn discharge_owned_handles(
        &mut self,
        block: BlockId,
        returned: ValueId,
    ) -> Result<(), Vec<BackendError>> {
        let drops = wit::owned_drops(&self.owned_handles, returned);
        self.owned_handles.clear();
        for instruction in drops {
            let span = instruction.span();
            self.append_instruction(block, instruction, span)?;
        }
        Ok(())
    }

    pub(super) fn append_instruction(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self.find_block_mut(block, span)?;
        if target.terminator.is_some() {
            return Err(vec![BackendError::invalid_ir(
                "P9 MIR lowering",
                span,
                "cannot append an instruction after a terminator",
            )]);
        }
        target.instructions.push(instruction);
        Ok(())
    }

    pub(super) fn wit_product_field(
        &mut self,
        block: BlockId,
        value: ValueId,
        field: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(ValueType::Ref(reference)) = self
            .values
            .iter()
            .find(|declaration| declaration.id == value)
            .map(|declaration| declaration.ty)
        else {
            return Err(unsupported_wit_record(span));
        };
        let HeapType::Index(type_index) = reference.heap else {
            return Err(unsupported_wit_record(span));
        };
        let field_shape = self
            .layout
            .product_field(type_index, field)
            .map_err(|error| layout_error(span, error))?;
        let field_type = self
            .layout
            .value_type(&field_shape)
            .map_err(|error| layout_error(span, error))?;
        let destination = self.fresh(field_type);
        self.append_instruction(
            block,
            Instruction::StructGet {
                destination,
                type_index,
                field,
                value,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    fn set_terminator(
        &mut self,
        block: BlockId,
        terminator: Terminator,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self.find_block_mut(block, span)?;
        if target.terminator.replace(terminator).is_some() {
            return Err(vec![BackendError::invalid_ir(
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
                vec![BackendError::invalid_ir(
                    "P9 MIR lowering",
                    span,
                    "basic block ID was not allocated",
                )]
            })
    }
}
