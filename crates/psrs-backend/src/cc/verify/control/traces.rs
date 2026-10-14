//! Preserve execution order and exceptional paths through structured CC.
use crate::cc::{Assignment, AssignmentKind as A, ValueId};
use crate::control::events::Event;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(super) struct Trace {
    pub(super) events: Vec<Event>,
    pub(super) casts: Vec<ValueId>,
    pub(super) trapped: bool,
    pub(super) aliases: HashMap<ValueId, ValueId>,
}

pub(super) fn paths(
    body: &[Assignment],
    input: Trace,
    executor: psrs_hir::SymbolId,
    definitions: &crate::cc::branch_facts::Definitions<'_>,
) -> Vec<Trace> {
    let mut current = vec![input];
    for assignment in body {
        let mut next = Vec::new();
        for mut trace in current {
            if trace.trapped {
                next.push(trace);
                continue;
            }
            match &assignment.kind {
                A::Unreachable => {
                    trace.trapped = true;
                    next.push(trace);
                }
                A::If {
                    then_assignments,
                    then_value,
                    else_assignments,
                    else_value,
                    condition,
                    ..
                } => {
                    for (branch, value, truth) in [
                        (then_assignments, then_value, true),
                        (else_assignments, else_value, false),
                    ] {
                        let mut selected = trace.clone();
                        if let Some((value, tag)) = crate::cc::branch_facts::equality_selection(
                            *condition,
                            truth,
                            definitions,
                            &selected.aliases,
                        ) {
                            selected.events.push(Event::Select { value, tag });
                        } else if let Some((value, tag)) =
                            crate::cc::branch_facts::equality_selection(
                                *condition,
                                !truth,
                                definitions,
                                &selected.aliases,
                            )
                        {
                            selected.events.push(Event::Reject {
                                value,
                                tags: vec![tag],
                            });
                        }
                        for mut path in paths(branch, selected, executor, definitions) {
                            path.aliases.insert(assignment.destination, *value);
                            next.push(path);
                        }
                    }
                }
                A::TagSwitch {
                    value,
                    cases,
                    default_assignments,
                    default_value,
                    ..
                } => {
                    for case in cases {
                        let mut selected = trace.clone();
                        selected.events.push(Event::Select {
                            value: *value,
                            tag: case.tag,
                        });
                        for mut path in paths(&case.assignments, selected, executor, definitions) {
                            path.aliases.insert(assignment.destination, case.value);
                            next.push(path);
                        }
                    }
                    trace.events.push(Event::Reject {
                        value: *value,
                        tags: cases.iter().map(|case| case.tag).collect(),
                    });
                    for mut path in paths(default_assignments, trace, executor, definitions) {
                        path.aliases.insert(assignment.destination, *default_value);
                        next.push(path);
                    }
                }
                kind => {
                    if matches!(kind, A::RepresentationCast { .. }) {
                        trace.casts.push(assignment.destination);
                        if let A::RepresentationCast { value, .. } = kind {
                            trace.events.push(Event::Refine {
                                result: assignment.destination,
                                source: crate::cc::value_identity::path_origin(
                                    *value,
                                    definitions,
                                    &trace.aliases,
                                ),
                            });
                        }
                    }
                    let event = match kind {
                        A::ArrayGet {
                            representation,
                            value,
                            index,
                            ..
                        } => Some(Event::Read {
                            repr: *representation,
                            cell: *value,
                            index: *index,
                            result: assignment.destination,
                        }),
                        A::ArraySet {
                            representation,
                            value,
                            index,
                            new_value,
                            ..
                        } => Some(Event::Store {
                            repr: *representation,
                            cell: *value,
                            index: *index,
                            value: *new_value,
                        }),
                        A::IndirectCall { .. } => Some(Event::Apply {
                            result: assignment.destination,
                        }),
                        A::DirectCall {
                            function,
                            arguments,
                            ..
                        } if *function == executor => Some(Event::Advance {
                            arguments: arguments.clone(),
                        }),
                        A::DirectCall {
                            function,
                            arguments,
                            ..
                        } => Some(Event::Direct {
                            function: *function,
                            arguments: arguments.clone(),
                        }),
                        A::RuntimeCall { .. } => Some(Event::OtherCall),
                        A::FunctionRef { .. }
                        | A::ProductNew { .. }
                        | A::VariantNew { .. }
                        | A::ArrayNew { .. }
                        | A::ArrayFill { .. }
                        | A::ArrayAppend { .. }
                        | A::ArrayClone { .. }
                        | A::StringToBytes { .. }
                        | A::BytesToString { .. } => Some(Event::Allocate {
                            result: assignment.destination,
                        }),
                        A::AggregateConvert { conversion, .. }
                            if !crate::cc::value_identity::transparent(&conversion.plan) =>
                        {
                            Some(Event::OtherEffect)
                        }
                        A::Primitive {
                            op: crate::cc::BinaryOp::IntQuot | crate::cc::BinaryOp::IntRem,
                            ..
                        } => Some(Event::Trap),
                        _ => None,
                    };
                    if let Some(event) = event {
                        trace.events.push(event)
                    }
                    next.push(trace);
                }
            }
        }
        current = next;
    }
    for path in &mut current {
        crate::control::events::canonicalize(&mut path.events, |value| {
            crate::cc::value_identity::path_origin(value, definitions, &path.aliases)
        });
    }
    current
}
