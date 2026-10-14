//! Discharge physical CFG exits into the shared root and persistent queue rules.
use crate::BackendError;
use crate::cc::{ControlHelperRole as R, ReprId};
use crate::control::execution::{self, ActionRegistry, Operands, Schema};
use crate::mir::layout::PlannedLayout;
use crate::mir::value_identity::{
    Definitions, definitions, loop_header, loop_parameters, path_origin,
};
use crate::mir::{Instruction as I, Module, Terminator as T};
use crate::types::ValueId;
use std::collections::HashMap;

pub(super) struct View<'a> {
    pub(super) definitions: &'a Definitions<'a>,
    pub(super) layout: &'a PlannedLayout,
    pub(super) aliases: &'a HashMap<ValueId, ValueId>,
    pub(super) raw: psrs_hir::SymbolId,
}

impl Operands for View<'_> {
    fn inputs(&self, value: ValueId) -> Vec<ValueId> {
        match self.definitions.get(&self.origin(value)).copied() {
            Some(I::StructNew { arguments, .. } | I::Call { arguments, .. }) => arguments.clone(),
            Some(I::ArrayNew { elements, .. }) => elements.clone(),
            Some(I::ClosureNew { captures, .. }) => captures.clone(),
            Some(I::ClosureCall {
                function,
                arguments,
                ..
            }) => std::iter::once(*function)
                .chain(arguments.iter().copied())
                .collect(),
            Some(I::StructGet { value, .. } | I::ArrayLen { value, .. }) => vec![*value],
            Some(I::ArrayNewFilled { length, value, .. }) => vec![*length, *value],
            Some(I::ArrayGet { value, index, .. }) => vec![*value, *index],
            _ => Vec::new(),
        }
    }
    fn tag(&self, value: ValueId) -> Option<(ReprId, ValueId)> {
        let I::StructGet {
            type_index,
            field: 0,
            value,
            ..
        } = self.definitions.get(&self.origin(value)).copied()?
        else {
            return None;
        };
        self.layout
            .representation_table()
            .representations
            .iter()
            .enumerate()
            .find_map(|(index, repr)| {
                let repr_id = ReprId(index as u32);
                (matches!(repr, crate::cc::Representation::Variant { .. })
                    && self.layout.repr_index(repr_id).ok() == Some(*type_index))
                .then_some((repr_id, self.origin(*value)))
            })
    }
    fn origin(&self, value: ValueId) -> ValueId {
        path_origin(value, self.definitions, self.aliases)
    }
    fn projection(
        &self,
        value: ValueId,
        repr: ReprId,
        case: Option<u32>,
    ) -> Option<(ValueId, u32)> {
        let expected = match case {
            Some(tag) => self.layout.variant_index(repr, tag),
            None => self.layout.repr_index(repr),
        }
        .ok()?;
        match self.definitions.get(&self.origin(value)).copied()? {
            I::StructGet {
                type_index,
                field,
                value,
                ..
            } if *type_index == expected => {
                let field = if case.is_some() {
                    field.checked_sub(1)?
                } else {
                    *field
                };
                Some((self.origin(*value), field))
            }
            _ => None,
        }
    }
    fn node(&self, value: ValueId, repr: ReprId, case: Option<u32>) -> Option<Vec<ValueId>> {
        let expected = match case {
            Some(tag) => self.layout.variant_index(repr, tag),
            None => self.layout.repr_index(repr),
        }
        .ok()?;
        match self.definitions.get(&self.origin(value)).copied()? {
            I::StructNew {
                type_index,
                arguments,
                ..
            } if *type_index == expected => {
                Some(arguments.get(usize::from(case.is_some())..)?.to_vec())
            }
            _ => None,
        }
    }
    fn application(&self, value: ValueId) -> Option<(ValueId, Vec<ValueId>)> {
        match self.definitions.get(&self.origin(value)).copied()? {
            I::ClosureCall {
                function,
                arguments,
                ..
            } => Some((*function, arguments.clone())),
            _ => None,
        }
    }
    fn raw_resumption(&self, value: ValueId) -> bool {
        matches!(self.definitions.get(&self.origin(value)).copied(), Some(I::ClosureNew { function, .. }) if *function == self.raw)
    }
}

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    let table = layout.representation_table();
    let total = crate::mir::effects::total_functions(module);
    for helper in &table.control_helpers {
        if helper.role() != R::Executor {
            continue;
        }
        let Some(function) = module
            .functions
            .iter()
            .find(|function| function.symbol == helper.function())
        else {
            continue;
        };
        let result = (|| {
            super::traces::entry(function, layout)?;
            let schema = Schema::checked(table)?;
            let raw = table
                .control_resumption
                .as_ref()
                .ok_or("execution has no raw resumption contract")?
                .function();
            let definitions = definitions(function);
            let header = loop_header(function);
            let exits = crate::mir::branch_facts::paths(
                function,
                header.unwrap_or(function.entry),
                header,
            )?;
            let carriers = [schema.computation, schema.queue, schema.context]
                .into_iter()
                .map(|repr| layout.repr_index(repr).map(|index| (index, repr)))
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(|_| "execution has no planned physical carrier")?;
            let normal = exits
                .iter()
                .filter(|path| !path.trapped)
                .map(|path| super::traces::events(function, layout, path))
                .collect::<Result<Vec<_>, _>>()?;
            let allowed = crate::control::events::carriers(table)?;
            let status = table
                .control_replay_schema()
                .ok_or("execution has no status schema")?
                .status;
            let mut coverage = ActionRegistry::default();
            for exit in exits {
                let view = View {
                    definitions: &definitions,
                    layout,
                    aliases: &exit.aliases,
                    raw,
                };
                let observed = super::traces::events(function, layout, &exit)?;
                if exit.trapped {
                    crate::control::events::exceptional(
                        &view, &observed, &normal, &allowed, status,
                    )?;
                    continue;
                }
                let facts = exit
                    .facts
                    .iter()
                    .filter_map(|((index, value), tag)| {
                        carriers
                            .get(index)
                            .map(|repr| ((*repr, view.origin(*value)), *tag))
                    })
                    .collect();
                let block = function
                    .blocks
                    .iter()
                    .find(|block| block.id == exit.block)
                    .ok_or("execution exit has no block")?;
                match block
                    .terminator
                    .as_ref()
                    .ok_or("execution path has no exit")?
                {
                    T::Jump { arguments, .. }
                    | T::ReturnCall {
                        function: _,
                        arguments,
                        ..
                    } => {
                        if matches!(block.terminator, Some(T::ReturnCall { function: target, .. }) if target != function.symbol)
                        {
                            return Err("execution must transition to its own executor");
                        }
                        execution::transition(
                            &schema,
                            &view,
                            &facts,
                            loop_parameters(function),
                            arguments,
                        )?;
                    }
                    T::Return { value, .. } => match definitions.get(&view.origin(*value)).copied()
                    {
                        Some(I::Call {
                            function: target,
                            arguments,
                            ..
                        }) if *target == function.symbol => execution::transition(
                            &schema,
                            &view,
                            &facts,
                            loop_parameters(function),
                            arguments,
                        )?,
                        _ => execution::publish(
                            &schema,
                            &view,
                            &facts,
                            loop_parameters(function),
                            *value,
                        )?,
                    },
                    _ => return Err("execution exit must return or transition to its executor"),
                }
                let next = match block
                    .terminator
                    .as_ref()
                    .ok_or("execution path has no exit")?
                {
                    T::Jump { arguments, .. } | T::ReturnCall { arguments, .. } => {
                        Some(arguments.as_slice())
                    }
                    T::Return { value, .. } => match definitions.get(&view.origin(*value)).copied()
                    {
                        Some(I::Call {
                            function: target,
                            arguments,
                            ..
                        }) if *target == function.symbol => Some(arguments.as_slice()),
                        _ => None,
                    },
                    _ => None,
                };
                execution::effects(
                    &schema,
                    &view,
                    &facts,
                    loop_parameters(function),
                    next,
                    &observed,
                    &total,
                )?;
                let result = match block.terminator.as_ref() {
                    Some(T::Return { value, .. }) => vec![*value],
                    _ => Vec::new(),
                };
                crate::control::events::allocations(&view, next.unwrap_or(&result), &observed)?;
                coverage.observe(&schema, &view, &facts, loop_parameters(function));
            }
            coverage.finish()
        })();
        if let Err(message) = result {
            errors.push(
                BackendError::invalid_ir("P9 MIR root and queue execution", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

impl crate::control::owned::OwnedOperands for View<'_> {
    fn integer(&self, value: ValueId) -> Option<i32> {
        crate::mir::value_identity::integer_constant(self.origin(value), self.definitions)
    }
    fn initial(&self, cell: ValueId, repr: ReprId) -> Option<ValueId> {
        let expected = self.layout.repr_index(repr).ok()?;
        match self.definitions.get(&self.origin(cell)).copied()? {
            I::ArrayNew {
                type_index,
                elements,
                ..
            } if *type_index == expected && elements.len() == 1 => Some(elements[0]),
            I::ArrayNewFilled {
                type_index,
                length,
                value,
                ..
            } if *type_index == expected && self.integer(*length) == Some(1) => Some(*value),
            _ => None,
        }
    }
    fn read(&self, value: ValueId, repr: ReprId) -> Option<(ValueId, i32)> {
        let expected = self.layout.repr_index(repr).ok()?;
        match self.definitions.get(&self.origin(value)).copied()? {
            I::ArrayGet {
                type_index,
                value,
                index,
                ..
            } if *type_index == expected => Some((self.origin(*value), self.integer(*index)?)),
            _ => None,
        }
    }
}
