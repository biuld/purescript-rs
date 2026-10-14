//! Discharge structured result paths into the shared root/queue step contract.
use crate::BackendError;
use crate::cc::branch_facts::{Definitions, tag_source};
use crate::cc::value_identity::{definitions, reference_origin, transparent};
use crate::cc::{
    AssignmentKind as A, ControlHelperRole as R, Function, ReprId, RepresentationTable, ValueId,
};
use crate::control::execution::{self, ActionRegistry, Facts, Operands, Schema};

pub(super) struct View<'a> {
    pub(super) definitions: Definitions<'a>,
    pub(super) raw: psrs_hir::SymbolId,
    pub(super) aliases: Option<&'a std::collections::HashMap<ValueId, ValueId>>,
}

impl Operands for View<'_> {
    fn inputs(&self, value: ValueId) -> Vec<ValueId> {
        match self.definitions.get(&self.origin(value)).copied() {
            Some(A::ProductNew { arguments, .. } | A::DirectCall { arguments, .. }) => {
                arguments.clone()
            }
            Some(A::VariantNew { fields, .. }) => fields.clone(),
            Some(A::ArrayNew { elements, .. }) => elements.clone(),
            Some(A::FunctionRef { captures, .. }) => captures.clone(),
            Some(A::IndirectCall {
                function,
                arguments,
                ..
            }) => std::iter::once(*function)
                .chain(arguments.iter().copied())
                .collect(),
            Some(
                A::ProductGet { value, .. }
                | A::VariantGet { value, .. }
                | A::VariantTag { value, .. }
                | A::ArrayLen { value, .. },
            ) => vec![*value],
            Some(A::ArrayFill { length, value, .. }) => vec![*length, *value],
            Some(A::ArrayGet { value, index, .. }) => vec![*value, *index],
            _ => Vec::new(),
        }
    }
    fn tag(&self, value: ValueId) -> Option<(ReprId, ValueId)> {
        match self.definitions.get(&self.origin(value)).copied()? {
            A::VariantTag {
                representation,
                value,
                ..
            } => Some((*representation, self.origin(*value))),
            _ => None,
        }
    }
    fn origin(&self, value: ValueId) -> ValueId {
        if let Some(aliases) = self.aliases {
            return crate::cc::value_identity::path_origin(value, &self.definitions, aliases);
        }
        reference_origin(value, |value| self.definitions.get(&value).copied())
    }
    fn projection(
        &self,
        value: ValueId,
        repr: ReprId,
        case: Option<u32>,
    ) -> Option<(ValueId, u32)> {
        match self.definitions.get(&self.origin(value)).copied()? {
            A::VariantGet {
                representation,
                case: tag,
                field,
                value,
                ..
            } if *representation == repr && case == Some(*tag) => {
                Some((self.origin(*value), *field))
            }
            A::ProductGet {
                representation,
                field,
                value,
                ..
            } if *representation == repr && case.is_none() => Some((self.origin(*value), *field)),
            _ => None,
        }
    }
    fn node(&self, value: ValueId, repr: ReprId, case: Option<u32>) -> Option<Vec<ValueId>> {
        match self.definitions.get(&self.origin(value)).copied()? {
            A::VariantNew {
                representation,
                case: tag,
                fields,
                ..
            } if *representation == repr && case == Some(*tag) => Some(fields.clone()),
            A::ProductNew {
                representation,
                arguments,
                ..
            } if *representation == repr && case.is_none() => Some(arguments.clone()),
            _ => None,
        }
    }
    fn application(&self, value: ValueId) -> Option<(ValueId, Vec<ValueId>)> {
        match self.definitions.get(&self.origin(value)).copied()? {
            A::IndirectCall {
                function,
                arguments,
                ..
            } => Some((*function, arguments.clone())),
            _ => None,
        }
    }
    fn raw_resumption(&self, value: ValueId) -> bool {
        matches!(self.definitions.get(&self.origin(value)).copied(), Some(A::FunctionRef { function, .. }) if *function == self.raw)
    }
}

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|helper| helper.role() == R::Executor && helper.function() == function.symbol)
    {
        return Ok(());
    }
    let result = (|| {
        let schema = Schema::checked(table)?;
        let view = View {
            definitions: definitions(&function.assignments),
            aliases: None,
            raw: table
                .control_resumption
                .as_ref()
                .ok_or("execution has no raw resumption contract")?
                .function(),
        };
        let mut coverage = ActionRegistry::default();
        walk(
            function,
            &schema,
            &view,
            function.result,
            Facts::new(),
            &mut std::collections::HashSet::new(),
            &mut coverage,
        )?;
        coverage.finish()
    })();
    result.map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC root and queue execution", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

fn walk(
    function: &Function,
    schema: &Schema,
    view: &View<'_>,
    value: ValueId,
    facts: Facts,
    active: &mut std::collections::HashSet<ValueId>,
    coverage: &mut ActionRegistry,
) -> Result<(), &'static str> {
    if !active.insert(value) {
        return Err("execution result path is cyclic");
    }
    let result = match view.definitions.get(&value).copied() {
        Some(A::RepresentationCast { value, .. }) => {
            walk(function, schema, view, *value, facts, active, coverage)
        }
        Some(A::AggregateConvert {
            value, conversion, ..
        }) if transparent(&conversion.plan) => {
            walk(function, schema, view, *value, facts, active, coverage)
        }
        Some(A::If {
            then_value,
            else_value,
            ..
        }) => {
            walk(
                function,
                schema,
                view,
                *then_value,
                facts.clone(),
                active,
                coverage,
            )?;
            walk(function, schema, view, *else_value, facts, active, coverage)
        }
        Some(A::TagSwitch {
            value: selector,
            cases,
            default_value,
            ..
        }) => {
            let selected = tag_source(*selector, &view.definitions);
            for case in cases {
                let mut branch = facts.clone();
                if let Some(key) = selected {
                    branch.insert(key, case.tag);
                }
                walk(function, schema, view, case.value, branch, active, coverage)?;
            }
            let mut fallback = facts;
            if let Some(key) = selected {
                fallback.remove(&key);
            }
            walk(
                function,
                schema,
                view,
                *default_value,
                fallback,
                active,
                coverage,
            )
        }
        Some(A::Unreachable) => Ok(()),
        Some(A::DirectCall {
            function: target,
            arguments,
            ..
        }) if *target == function.symbol => {
            execution::transition(schema, view, &facts, &function.parameters, arguments)?;
            coverage.observe(schema, view, &facts, &function.parameters);
            Ok(())
        }
        _ => {
            execution::publish(schema, view, &facts, &function.parameters, value)?;
            coverage.observe(schema, view, &facts, &function.parameters);
            Ok(())
        }
    };
    active.remove(&value);
    result
}

impl crate::control::owned::OwnedOperands for View<'_> {
    fn integer(&self, value: ValueId) -> Option<i32> {
        match self.definitions.get(&self.origin(value)).copied()? {
            A::Constant(value) => Some(*value),
            _ => None,
        }
    }
    fn initial(&self, cell: ValueId, repr: ReprId) -> Option<ValueId> {
        match self.definitions.get(&self.origin(cell)).copied()? {
            A::ArrayNew {
                representation,
                elements,
                ..
            } if *representation == repr && elements.len() == 1 => Some(elements[0]),
            A::ArrayFill {
                representation,
                length,
                value,
                ..
            } if *representation == repr && self.integer(*length) == Some(1) => Some(*value),
            _ => None,
        }
    }
    fn read(&self, value: ValueId, repr: ReprId) -> Option<(ValueId, i32)> {
        match self.definitions.get(&self.origin(value)).copied()? {
            A::ArrayGet {
                representation,
                value,
                index,
                ..
            } if *representation == repr => Some((self.origin(*value), self.integer(*index)?)),
            _ => None,
        }
    }
}

mod effects;

pub(super) fn verify_effects(
    function: &Function,
    table: &RepresentationTable,
    functions: Option<&std::collections::HashMap<psrs_hir::SymbolId, &Function>>,
) -> Result<(), Vec<BackendError>> {
    effects::verify(function, table, functions)
}
