use crate::cc::{self, AssignmentKind, BinaryOp};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, NumericOp, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use crate::{BackendError, mir};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScalarHelpers {
    pub(super) int_div: Option<SymbolId>,
    pub(super) int_mod: Option<SymbolId>,
}

impl ScalarHelpers {
    pub(super) fn binary_instruction(
        &self,
        op: BinaryOp,
        destination: ValueId,
        left: ValueId,
        right: ValueId,
        span: TextRange,
    ) -> Result<Instruction, Vec<BackendError>> {
        let helper = match op {
            BinaryOp::IntDiv => self.int_div,
            BinaryOp::IntMod => self.int_mod,
            _ => None,
        };
        if let Some(function) = helper {
            return Ok(Instruction::Call {
                destination,
                function,
                arguments: vec![left, right],
                span,
            });
        }
        let op = mir::NumericOp::try_from(op).map_err(|unlowered| {
            vec![BackendError::invalid_ir(
                "P9 MIR lowering",
                span,
                format!("missing MIR helper for scalar operation {unlowered:?}"),
            )]
        })?;
        Ok(Instruction::Primitive {
            destination,
            op,
            left,
            right,
            span,
        })
    }
}

pub(super) fn lower_scalar_helpers(
    module: &cc::Module,
    first_function_id: u32,
) -> (ScalarHelpers, Vec<Function>) {
    let needs_int_div = module
        .functions
        .iter()
        .any(|function| contains_operation(&function.assignments, BinaryOp::IntDiv));
    let needs_int_mod = module
        .functions
        .iter()
        .any(|function| contains_operation(&function.assignments, BinaryOp::IntMod));
    let mut used_symbols = module
        .functions
        .iter()
        .map(|function| function.symbol)
        .chain(module.externals.iter().map(|external| external.symbol))
        .collect::<HashSet<_>>();
    // Intrinsic symbols may be allocated downward from `u32::MAX`; never take
    // one the canonical ABI or codec reserves.
    used_symbols.extend(crate::abi::RESERVED_ABI_SYMBOLS);
    let symbol_module = module
        .functions
        .first()
        .map_or(ModuleId::INTRINSICS, |function| function.symbol.module);
    let int_div = needs_int_div.then(|| allocate_symbol(symbol_module, &mut used_symbols));
    let int_mod = needs_int_mod.then(|| allocate_symbol(symbol_module, &mut used_symbols));
    let helpers = ScalarHelpers { int_div, int_mod };
    let mut functions = Vec::new();
    let mut next_id = first_function_id;
    if let Some(symbol) = int_div {
        functions.push(euclidean_helper(
            FunctionId(next_id),
            symbol,
            EuclideanOperation::Divide,
            module.span,
        ));
        next_id += 1;
    }
    if let Some(symbol) = int_mod {
        functions.push(euclidean_helper(
            FunctionId(next_id),
            symbol,
            EuclideanOperation::Modulo,
            module.span,
        ));
    }
    (helpers, functions)
}

fn allocate_symbol(module: ModuleId, used: &mut HashSet<SymbolId>) -> SymbolId {
    let mut index = u32::MAX;
    loop {
        let symbol = SymbolId::new(module, index);
        if used.insert(symbol) {
            return symbol;
        }
        index = index
            .checked_sub(1)
            .expect("generated scalar helper symbol space exhausted");
    }
}

fn contains_operation(assignments: &[cc::Assignment], needle: BinaryOp) -> bool {
    assignments.iter().any(|assignment| match &assignment.kind {
        AssignmentKind::Primitive { op, .. } => *op == needle,
        AssignmentKind::If {
            then_assignments,
            else_assignments,
            ..
        } => {
            contains_operation(then_assignments, needle)
                || contains_operation(else_assignments, needle)
        }
        AssignmentKind::TagSwitch {
            cases,
            default_assignments,
            ..
        } => {
            cases
                .iter()
                .any(|case| contains_operation(&case.assignments, needle))
                || contains_operation(default_assignments, needle)
        }
        _ => false,
    })
}

#[derive(Clone, Copy)]
enum EuclideanOperation {
    Divide,
    Modulo,
}

fn euclidean_helper(
    id: FunctionId,
    symbol: SymbolId,
    operation: EuclideanOperation,
    span: TextRange,
) -> Function {
    let a = ValueId(0);
    let b = ValueId(1);
    let remainder = ValueId(2);
    let zero = ValueId(3);
    let nonzero_remainder = ValueId(4);
    let remainder_negative = ValueId(5);
    let divisor_negative = ValueId(6);
    let signs_differ = ValueId(7);
    let should_adjust = ValueId(8);
    let result = ValueId(9);
    let mut values = (0..=9)
        .map(|id| ValueDecl {
            id: ValueId(id),
            ty: ValueType::I32,
        })
        .collect::<Vec<_>>();
    values[4].ty = ValueType::Boolean;
    values[5].ty = ValueType::Boolean;
    values[6].ty = ValueType::Boolean;
    values[7].ty = ValueType::Boolean;
    values[8].ty = ValueType::Boolean;

    let mut entry_instructions = vec![
        primitive(remainder, NumericOp::I32RemS, a, b, span),
        Instruction::Constant {
            destination: zero,
            value: 0,
            span,
        },
        primitive(nonzero_remainder, NumericOp::I32Ne, remainder, zero, span),
        primitive(remainder_negative, NumericOp::I32LtS, remainder, zero, span),
        primitive(divisor_negative, NumericOp::I32LtS, b, zero, span),
        primitive(
            signs_differ,
            NumericOp::BoolNe,
            remainder_negative,
            divisor_negative,
            span,
        ),
        primitive(
            should_adjust,
            NumericOp::BoolAnd,
            nonzero_remainder,
            signs_differ,
            span,
        ),
    ];
    let (adjust_value, unchanged_value, name) = match operation {
        EuclideanOperation::Divide => {
            let quotient = ValueId(10);
            let one = ValueId(11);
            let adjusted = ValueId(12);
            let unchanged = ValueId(13);
            values.extend([
                ValueDecl {
                    id: quotient,
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: one,
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: adjusted,
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: unchanged,
                    ty: ValueType::I32,
                },
            ]);
            let adjusted_instructions = vec![
                primitive(quotient, NumericOp::I32DivS, a, b, span),
                Instruction::Constant {
                    destination: one,
                    value: 1,
                    span,
                },
                primitive(adjusted, NumericOp::I32Sub, quotient, one, span),
            ];
            let unchanged_instructions = vec![primitive(unchanged, NumericOp::I32DivS, a, b, span)];
            (
                (adjusted, adjusted_instructions),
                (unchanged, unchanged_instructions),
                "__psrs_euclidean_int_div",
            )
        }
        EuclideanOperation::Modulo => {
            let adjusted = ValueId(10);
            values.push(ValueDecl {
                id: adjusted,
                ty: ValueType::I32,
            });
            (
                (
                    adjusted,
                    vec![primitive(adjusted, NumericOp::I32Add, remainder, b, span)],
                ),
                (remainder, Vec::new()),
                "__psrs_euclidean_int_mod",
            )
        }
    };
    let merge = BlockId(3);
    Function {
        id,
        symbol,
        name: name.into(),
        parameters: vec![a, b],
        values,
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: std::mem::take(&mut entry_instructions),
                terminator: Some(Terminator::Branch {
                    condition: should_adjust,
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span,
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: adjust_value.1,
                terminator: Some(Terminator::Jump {
                    target: merge,
                    arguments: vec![adjust_value.0],
                    span,
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: unchanged_value.1,
                terminator: Some(Terminator::Jump {
                    target: merge,
                    arguments: vec![unchanged_value.0],
                    span,
                }),
            },
            BasicBlock {
                id: merge,
                parameters: vec![result],
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: result,
                    span,
                }),
            },
        ],
        result,
        result_type: ValueType::I32,
        span,
    }
}

fn primitive(
    destination: ValueId,
    op: NumericOp,
    left: ValueId,
    right: ValueId,
    span: TextRange,
) -> Instruction {
    Instruction::Primitive {
        destination,
        op,
        left,
        right,
        span,
    }
}
