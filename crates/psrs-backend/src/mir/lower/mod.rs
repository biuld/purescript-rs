use super::BoundWasiImport;
use super::layout::{LayoutError, PlannedLayout};
use super::literals::StringLiterals;
use super::wit;
use super::{BasicBlock, BlockId, Function, Terminator};
use crate::BackendError;
use crate::cc::{self, AssignmentKind};
use crate::mir::instruction::Instruction;
use crate::types::{FunctionId, HeapType, ValueDecl, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;

mod aggregate;
mod array_assignments;
mod assignment_array;
mod assignment_string;
mod assignments;
mod conversion_helpers;
mod runtime_call;
mod storage_call;
mod tail;
mod variant;
mod wit_call;
pub(super) use conversion_helpers::ConversionHelpers;
#[cfg(test)]
mod wit_tests;

fn layout_error(span: TextRange, error: LayoutError) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        format!("invalid concrete layout request: {error}"),
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
    runtime: &super::runtime::RuntimeContext,
    layout: &PlannedLayout,
    conversion_helpers: Option<&mut ConversionHelpers>,
    literals: Option<&mut StringLiterals>,
    logical_state_source: Option<std::sync::Arc<cc::Module>>,
    target: crate::capability::TargetCapabilities,
) -> Result<Function, Vec<BackendError>> {
    let entry = BlockId(0);
    // Removed logical identities remain reserved while their source-to-MIR
    // relation is checked; helper temporaries must not reuse State identities.
    let logical_values = logical_state_source
        .as_ref()
        .and_then(|module| {
            module
                .functions
                .iter()
                .find(|function| function.symbol == source.symbol)
        })
        .unwrap_or(source);
    let mut lowerer = FunctionLowerer {
        owner: source.symbol,
        runtime: Some(runtime),
        runtime_calls: Vec::new(),
        wit_calls: Vec::new(),
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
        next_value: logical_values
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .map_or(0, |max| max + 1),
        wit_imports,
        layout,
        conversion_helpers,
        literals,
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
    let mut function = Function {
        state: None,
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
    for plan in &lowerer.wit_calls {
        plan.verify(&function)?;
    }
    if let Some(source) = logical_state_source {
        function.state = Some(super::state::DependencyFlow::checked_with_runtime(
            source,
            &function,
            lowerer.runtime_calls,
            lowerer.wit_calls,
        )?);
    } else if !lowerer.runtime_calls.is_empty() {
        return Err(vec![BackendError::invalid_ir(
            "P9 runtime projection",
            function.span,
            "raw runtime invocation has no owning dependency flow",
        )]);
    }
    tail::mark_tail(&mut function, target)?;
    wit::verify_function(&function, wit_imports)?;
    Ok(function)
}
pub(super) struct FunctionLowerer<'a> {
    owner: SymbolId,
    runtime: Option<&'a super::runtime::RuntimeContext>,
    runtime_calls: Vec<super::state::RuntimeInvocation>,
    wit_calls: Vec<super::wit::CallPlan>,
    next_block: u32,
    blocks: Vec<BasicBlock>,
    values: Vec<ValueDecl>,
    next_value: u32,
    wit_imports: &'a HashMap<SymbolId, BoundWasiImport>,
    pub(in crate::mir) layout: &'a PlannedLayout,
    conversion_helpers: Option<&'a mut ConversionHelpers>,
    literals: Option<&'a mut StringLiterals>,
}

impl FunctionLowerer<'_> {
    pub(super) fn value_type(&self, id: ValueId) -> Option<ValueType> {
        self.values
            .iter()
            .find(|decl| decl.id == id)
            .map(|decl| decl.ty)
    }

    /// The concrete GC type of a representation handle.
    pub(super) fn resolved_repr_index(
        &self,
        repr: crate::cc::ReprId,
    ) -> Option<crate::types::DefinedTypeId> {
        self.layout.repr_index(repr).ok()
    }

    /// The recursive guest layout of a value shape, read from the representation
    /// table this layout was planned from.
    pub(super) fn resolved_guest_layout(
        &self,
        shape: crate::cc::ValueShape,
    ) -> Option<crate::cc::GuestLayout> {
        crate::cc::guest_layout(shape, self.layout.representation_table())
    }

    /// The concrete MIR value type of a value shape.
    pub(super) fn resolved_value_type(
        &self,
        shape: &crate::cc::ValueShape,
    ) -> Option<crate::types::ValueType> {
        self.layout.value_type(shape).ok()
    }

    pub(super) fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
    }
    pub(in crate::mir) fn new_block(&mut self, parameters: Vec<ValueId>) -> BlockId {
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

    /// The boxed-integer representation, when the module needs it.
    pub(in crate::mir) fn wit_boxed_integer(&self) -> Option<crate::types::DefinedTypeId> {
        self.layout.boxed_integer_index()
    }

    /// The boxed-number representation, when the module needs it.
    pub(in crate::mir) fn wit_boxed_number(&self) -> Option<crate::types::DefinedTypeId> {
        self.layout.boxed_number_index()
    }

    /// The concrete GC string type, when the module needs it.
    pub(in crate::mir) fn wit_string_index(&self) -> Option<crate::types::DefinedTypeId> {
        self.layout.string_index()
    }

    /// The concrete MIR type of a source variant case field.
    pub(in crate::mir) fn wit_case_field_type(
        &self,
        representation: crate::cc::ReprId,
        case: u32,
        field: u32,
    ) -> Option<crate::types::ValueType> {
        let shape = self
            .layout
            .variant_field(representation, case, field)
            .ok()?;
        self.layout.value_type(&shape).ok()
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

    pub(in crate::mir) fn set_terminator(
        &mut self,
        block: BlockId,
        terminator: Terminator,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self.find_block_mut(block, span)?;
        if target.terminator.is_some() {
            return Err(vec![BackendError::invalid_ir(
                "P9 MIR lowering",
                span,
                "basic block already has a terminator",
            )]);
        }
        target.terminator = Some(
            if matches!(
                target.instructions.last(),
                Some(Instruction::Unreachable { .. })
            ) {
                // A static trap has no normal return or jump successor.
                Terminator::Trap { span }
            } else {
                terminator
            },
        );
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
