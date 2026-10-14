//! A source return or operation applies its own scoped clause.
//!
//! Source context construction retains the clauses at fixed fields
//! (`context_entry`); this verifier checks that interpreting a source `Return`
//! or `Perform` actually applies the matching clause of the current scoped
//! context, rather than the wrong field, another frame's field or a boundary.
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::provenance::{
    ClauseKind, FinishProjection, clause_operand_relation, clause_selection_relation,
};

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|helper| helper.function() == function.symbol && helper.role() == R::Executor)
    {
        return Ok(());
    }
    if function.parameters.len() != 6 {
        return Err(error(
            function,
            "clause selection has an invalid executor contract",
        ));
    }
    let schema = table
        .control_checkpoint_schema()
        .ok_or_else(|| error(function, "clause selection has no checked schema"))?;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin = |value| {
        crate::cc::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
    };
    let context = function.parameters[5];
    let Some(A::TagSwitch { cases, .. }) = definitions.get(&origin(function.result)).copied()
    else {
        // The dispatch verifier owns the result-shape contract.
        return Ok(());
    };
    for case in cases {
        let kind = if case.tag == ClauseKind::Return.source_tag() {
            ClauseKind::Return
        } else if case.tag == ClauseKind::Operation.source_tag() {
            ClauseKind::Operation
        } else {
            continue;
        };
        // Check every applied clause projection and require the scoped branch
        // to consume its complete application result at the next transition.
        let mut pending = vec![case.assignments.as_slice()];
        let mut steps = 0;
        while let Some(body) = pending.pop() {
            for assignment in body {
                match &assignment.kind {
                    A::IndirectCall {
                        function: callee, ..
                    } => {
                        let clause = match definitions.get(&origin(*callee)).copied() {
                            Some(A::VariantGet {
                                representation,
                                case: 1,
                                field,
                                value,
                                ..
                            }) if *representation == schema.context => Some(FinishProjection {
                                owner: origin(*value),
                                field: *field,
                            }),
                            _ => None,
                        };
                        if clause.is_some() {
                            clause_selection_relation(kind, origin(context), clause)
                                .map_err(|message| error(function, message))?;
                        }
                    }
                    A::TagSwitch {
                        value,
                        cases,
                        default_assignments,
                        ..
                    } => {
                        if crate::cc::branch_facts::tag_source(*value, &definitions)
                            == Some((schema.context, origin(context)))
                        {
                            let scoped =
                                cases.iter().find(|case| case.tag == 1).ok_or_else(|| {
                                    error(function, "source step has no scoped clause path")
                                })?;
                            let Some(A::DirectCall {
                                function: target,
                                arguments,
                                ..
                            }) = definitions.get(&origin(scoped.value)).copied()
                            else {
                                return Err(error(
                                    function,
                                    "source clause result must enter the next executor step",
                                ));
                            };
                            if *target != function.symbol || arguments.len() != 6 {
                                return Err(error(
                                    function,
                                    "source clause result must enter the next executor step",
                                ));
                            }
                            if origin(arguments[0]) != origin(function.parameters[0])
                                || origin(arguments[1]) != origin(function.parameters[1])
                            {
                                return Err(error(
                                    function,
                                    "source clause transition must preserve its state and root handler",
                                ));
                            }
                            let mut result = origin(arguments[2]);
                            let mut payload = None;
                            let mut resumption = None;
                            for _ in 0..(if kind == ClauseKind::Return { 1 } else { 2 }) {
                                let Some(A::IndirectCall {
                                    function: callee,
                                    arguments,
                                    ..
                                }) = definitions.get(&result).copied()
                                else {
                                    return Err(error(
                                        function,
                                        "source clause result must enter the next executor step",
                                    ));
                                };
                                if arguments.len() != 1
                                    || !scoped.assignments.iter().any(|a| a.destination == result)
                                {
                                    return Err(error(
                                        function,
                                        "source clause application must execute in its scoped step",
                                    ));
                                }
                                payload = Some(origin(arguments[0]));
                                resumption.get_or_insert(origin(arguments[0]));
                                result = origin(*callee);
                            }
                            let projection =
                                |value, repr, tag| match definitions.get(&value).copied() {
                                    Some(A::VariantGet {
                                        representation,
                                        case,
                                        field,
                                        value,
                                        ..
                                    }) if *representation == repr && *case == tag => {
                                        Some(FinishProjection {
                                            owner: origin(*value),
                                            field: *field,
                                        })
                                    }
                                    _ => None,
                                };
                            clause_selection_relation(
                                kind,
                                origin(context),
                                projection(result, schema.context, 1),
                            )
                            .map_err(|message| error(function, message))?;
                            clause_operand_relation(
                                origin(function.parameters[2]),
                                projection(
                                    payload.unwrap(),
                                    schema.computation,
                                    kind.source_tag() as u32,
                                ),
                            )
                            .map_err(|message| error(function, message))?;
                            if kind == ClauseKind::Operation
                                && !matches!(definitions.get(&resumption.unwrap()).copied(), Some(A::FunctionRef { function: target, .. })
                                    if table.control_resumption.as_ref().is_some_and(|raw| raw.function() == *target))
                            {
                                return Err(error(
                                    function,
                                    "source operation clause must receive its raw resumption",
                                ));
                            }
                            steps += 1;
                        }
                        pending.extend(cases.iter().map(|case| case.assignments.as_slice()));
                        pending.push(default_assignments);
                    }
                    A::If {
                        then_assignments,
                        else_assignments,
                        ..
                    } => pending.extend([then_assignments.as_slice(), else_assignments.as_slice()]),
                    _ => {}
                }
            }
        }
        if steps == 0 {
            return Err(error(function, "source step has no scoped clause path"));
        }
    }
    Ok(())
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC clause selection", function.span, message)
            .with_module(function.symbol.module),
    ]
}
