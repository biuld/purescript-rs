use super::super::Function;
use super::*;
use crate::cc;
use crate::types::FunctionId;

pub(crate) fn lower_function(
    source: &cc::Function,
    id: FunctionId,
    layout: &LinearMemoryLayout,
    wit_imports: &HashMap<SymbolId, crate::abi::BoundWasiImport>,
    scalar_helpers: &ScalarHelpers,
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
        scalar_helpers,
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
