//! Recheck lifecycle ownership after physical projection and loop conversion.
use super::{Function, Instruction as I, Module, Terminator as T};
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control_provenance::{FinishProjection, finish_relation};
use crate::types::{DefinedTypeId, ValueId};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else {
        return;
    };
    let table = layout.representation_table();
    let Some(executor) = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
    else {
        return;
    };
    if !module
        .functions
        .iter()
        .any(|function| function.symbol == executor.function())
    {
        return;
    }
    let Some(sequence) = table.control_sequence() else {
        if table
            .control_helpers
            .iter()
            .any(|helper| helper.role() == R::Executor)
        {
            errors.push(BackendError::invalid_ir(
                "P9 MIR control provenance",
                module.span,
                "control executor has no checked sequence schema",
            ));
        }
        return;
    };
    let Ok(finish_type) = layout.variant_index(sequence, 3) else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR control provenance",
            module.span,
            "control finish schema has no physical layout",
        ));
        return;
    };
    for function in &module.functions {
        if !table
            .control_helpers
            .iter()
            .any(|helper| helper.role() == R::Executor && helper.function() == function.symbol)
        {
            continue;
        }
        let header = super::value_identity::loop_header(function);
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                let I::Call {
                    contract: Some(contract),
                    arguments,
                    span,
                    ..
                } = instruction
                else {
                    continue;
                };
                if !table
                    .control_helpers
                    .get(contract.0 as usize)
                    .is_some_and(|helper| helper.role() == R::Close)
                {
                    continue;
                }
                let next = block.instructions[index + 1..]
                    .iter()
                    .find_map(|instruction| match instruction {
                        I::Call {
                            contract: Some(contract),
                            arguments,
                            ..
                        } if table
                            .control_helpers
                            .get(contract.0 as usize)
                            .is_some_and(|helper| helper.role() == R::Executor) =>
                        {
                            Some(arguments.as_slice())
                        }
                        _ => None,
                    })
                    .or_else(|| match &block.terminator {
                        Some(T::ReturnCall {
                            contract: Some(contract),
                            arguments,
                            ..
                        }) if table
                            .control_helpers
                            .get(contract.0 as usize)
                            .is_some_and(|helper| helper.role() == R::Executor) =>
                        {
                            Some(arguments.as_slice())
                        }
                        Some(T::Jump {
                            target, arguments, ..
                        }) if Some(*target) == header => Some(arguments.as_slice()),
                        _ => None,
                    });
                let check = match next {
                    Some(next) if arguments.len() == 1 && next.len() == 6 => finish_relation(
                        projection(arguments[0], function, finish_type),
                        projection(next[3], function, finish_type),
                        projection(next[4], function, finish_type),
                    ),
                    _ => Err("control finish has no checked executor transition"),
                };
                if let Err(message) = check {
                    errors.push(
                        BackendError::invalid_ir("P9 MIR control provenance", *span, message)
                            .with_module(function.symbol.module),
                    );
                }
            }
            let transitions = block
                .instructions
                .iter()
                .filter_map(|instruction| match instruction {
                    I::Call {
                        contract: Some(contract),
                        arguments,
                        span,
                        ..
                    } if table
                        .control_helpers
                        .get(contract.0 as usize)
                        .is_some_and(|helper| helper.role() == R::Executor) =>
                    {
                        Some((arguments.as_slice(), *span))
                    }
                    _ => None,
                })
                .chain(block.terminator.iter().filter_map(|terminator| {
                    match terminator {
                        T::ReturnCall {
                            contract: Some(contract),
                            arguments,
                            span,
                            ..
                        } if table
                            .control_helpers
                            .get(contract.0 as usize)
                            .is_some_and(|helper| helper.role() == R::Executor) =>
                        {
                            Some((arguments.as_slice(), *span))
                        }
                        T::Jump {
                            target,
                            arguments,
                            span,
                        } if Some(*target) == header => Some((arguments.as_slice(), *span)),
                        _ => None,
                    }
                }));
            for (next, span) in transitions {
                if next.len() != 6
                    || !projection(next[3], function, finish_type)
                        .is_some_and(|projection| projection.field == 1)
                {
                    continue;
                }
                let close =
                    block
                        .instructions
                        .iter()
                        .rev()
                        .find_map(|instruction| match instruction {
                            I::Call {
                                contract: Some(contract),
                                arguments,
                                ..
                            } if table
                                .control_helpers
                                .get(contract.0 as usize)
                                .is_some_and(|helper| helper.role() == R::Close) =>
                            {
                                arguments.first().copied()
                            }
                            _ => None,
                        });
                let check = match close {
                    Some(close) => finish_relation(
                        projection(close, function, finish_type),
                        projection(next[3], function, finish_type),
                        projection(next[4], function, finish_type),
                    ),
                    None => Err("control finish transition does not close its frame"),
                };
                if let Err(message) = check {
                    errors.push(
                        BackendError::invalid_ir("P9 MIR control provenance", span, message)
                            .with_module(function.symbol.module),
                    );
                }
            }
        }
    }
}

fn projection(
    value: ValueId,
    function: &Function,
    finish_type: DefinedTypeId,
) -> Option<FinishProjection> {
    let definitions = super::value_identity::definitions(function);
    let origin = |value| super::value_identity::reference_origin(value, &definitions);
    match definitions.get(&origin(value)).copied()? {
        I::StructGet {
            type_index,
            field,
            value: owner,
            ..
        } if *type_index == finish_type && *field > 0 => Some(FinishProjection {
            owner: origin(*owner),
            field: field - 1,
        }),
        _ => None,
    }
}
