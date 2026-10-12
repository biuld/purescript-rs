//! Run factories establish one fresh root and close it before publishing a result.
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control_provenance::root_extent_relation;
use psrs_hir::Intrinsic;
use std::collections::HashMap;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    let Some(schema) = table.control_replay_schema() else {
        return Ok(());
    };
    let context = table
        .control_checkpoint_schema()
        .ok_or_else(|| error(function, "control run has no checked root schema"))?
        .context;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin = |value| {
        crate::cc::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
    };
    let role = |id: crate::cc::ControlHelperId| {
        table
            .control_helpers
            .get(id.0 as usize)
            .map(|helper| helper.role())
    };
    let constant = |value, expected| matches!(definitions.get(&origin(value)).copied(), Some(A::Constant(value)) if *value == expected);
    if table
        .control_helpers
        .iter()
        .any(|helper| helper.function() == function.symbol && helper.role() == R::Start)
    {
        if !function.parameters.is_empty() {
            return Err(error(
                function,
                "control Start has an invalid parameter contract",
            ));
        }
        let Some(A::VariantNew {
            representation,
            case: 0,
            fields,
            ..
        }) = definitions.get(&origin(function.result)).copied()
        else {
            return Err(error(
                function,
                "control Start must return a freshly allocated root extent",
            ));
        };
        if *representation != schema.boundary || fields.len() != 1 {
            return Err(error(
                function,
                "control Start must return a freshly allocated root extent",
            ));
        }
        let live = match definitions.get(&origin(fields[0])).copied() {
            Some(A::ArrayNew {
                representation,
                elements,
                ..
            }) => {
                *representation == schema.status && elements.len() == 1 && constant(elements[0], 0)
            }
            Some(A::ArrayFill {
                representation,
                length,
                value,
                ..
            }) => *representation == schema.status && constant(*length, 1) && constant(*value, 0),
            _ => false,
        };
        if !live {
            return Err(error(
                function,
                "control Start needs a fresh one-element live status cell",
            ));
        }
    }
    let mut owners = HashMap::<_, Vec<(_, Vec<(crate::cc::ValueId, i64)>)>>::new();
    let mut pending = vec![(function.assignments.as_slice(), Vec::new())];
    while let Some((body, path)) = pending.pop() {
        for (index, assignment) in body.iter().enumerate() {
            match &assignment.kind {
                A::TagSwitch {
                    cases,
                    default_assignments,
                    ..
                } => {
                    for case in cases {
                        let mut child = path.clone();
                        child.push((assignment.destination, i64::from(case.tag)));
                        pending.push((case.assignments.as_slice(), child));
                    }
                    let mut child = path.clone();
                    child.push((assignment.destination, i64::MIN));
                    pending.push((default_assignments.as_slice(), child));
                }
                A::If {
                    then_assignments,
                    else_assignments,
                    ..
                } => {
                    for (body, arm) in [
                        (then_assignments.as_slice(), 0),
                        (else_assignments.as_slice(), 1),
                    ] {
                        let mut child = path.clone();
                        child.push((assignment.destination, arm));
                        pending.push((body, child));
                    }
                }
                A::ControlEvidence {
                    contract,
                    implementation,
                    ..
                } if table
                    .control_contracts
                    .get(contract.0 as usize)
                    .is_some_and(|proof| proof.operation() == Intrinsic::ControlRun) =>
                {
                    let implementation = origin(*implementation);
                    let call_index = body[..index]
                        .iter()
                        .position(|assignment| assignment.destination == implementation)
                        .ok_or_else(|| {
                            error(function, "control run has no local executor invocation")
                        })?;
                    let A::DirectCall {
                        contract: Some(id),
                        arguments,
                        ..
                    } = &body[call_index].kind
                    else {
                        return Err(error(
                            function,
                            "control run has no local executor invocation",
                        ));
                    };
                    if role(*id) != Some(R::Executor) || arguments.len() != 6 {
                        return Err(error(
                            function,
                            "control run has no local executor invocation",
                        ));
                    }
                    if !matches!(definitions.get(&origin(arguments[3])).copied(), Some(A::VariantNew { representation, case: 0, fields, .. }) if *representation == schema.sequence && fields.is_empty())
                    {
                        return Err(error(
                            function,
                            "control run must start with an empty root queue",
                        ));
                    }
                    let extent = origin(arguments[4]);
                    let previous = owners.entry(extent).or_default();
                    if previous.iter().any(|(call, previous_path)| {
                        *call != implementation
                            && !previous_path.iter().any(|(branch, arm)| {
                                path.iter().any(|(current_branch, current_arm)| {
                                    branch == current_branch && arm != current_arm
                                })
                            })
                    }) {
                        return Err(error(
                            function,
                            "control run factories must not reuse another invocation's root extent",
                        ));
                    }
                    previous.push((implementation, path.clone()));
                    if !matches!(definitions.get(&extent).copied(), Some(A::DirectCall { contract: Some(id), arguments, .. }) if role(*id) == Some(R::Start) && arguments.is_empty())
                    {
                        return Err(error(
                            function,
                            "control run must start with a fresh Start extent",
                        ));
                    }
                    let Some(A::VariantNew {
                        representation,
                        case: 0,
                        fields,
                        ..
                    }) = definitions.get(&origin(arguments[5])).copied()
                    else {
                        return Err(error(
                            function,
                            "control run must install a root dispatch context",
                        ));
                    };
                    if *representation != context || fields.len() != 1 {
                        return Err(error(
                            function,
                            "control run must install a root dispatch context",
                        ));
                    }
                    root_extent_relation(extent, origin(fields[0]))
                        .map_err(|message| error(function, message))?;
                    if !body[call_index + 1..index].iter().any(|assignment| matches!(&assignment.kind,
                        A::DirectCall { contract: Some(id), arguments, .. } if role(*id) == Some(R::Close) && arguments.len() == 1 && origin(arguments[0]) == extent)) {
                        return Err(error(function, "control run must close its root after execution and before publishing its result"));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC control root", function.span, message)
            .with_module(function.symbol.module),
    ]
}
