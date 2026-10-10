//! P9's private instruction view: immutable logical CC remains the authority.
use super::*;
use crate::cc::{AssignmentKind as A, RefShape, Representation, ValueConversion, ValueShape};
use std::collections::{HashMap, HashSet};

pub(in crate::mir) struct ProjectedInput {
    pub physical: cc::Module,
    pub logical: Arc<cc::Module>,
    pub inventory: Inventory,
}

pub(in crate::mir) fn project(
    module: cc::Module,
    runtime: &[crate::RuntimeBinding],
) -> Result<ProjectedInput, Vec<BackendError>> {
    let mut runtime_symbols = HashSet::new();
    for binding in runtime {
        crate::bindings::checked_runtime_call(binding, &module)?;
        runtime_symbols.insert(binding.symbol);
    }
    let logical = Arc::new(module);
    let inventory = Inventory::checked(logical.clone())?;
    let mut physical = (*logical).clone();
    let steps = logical
        .representations
        .representations
        .iter()
        .enumerate()
        .filter_map(|(id, representation)| {
            let Representation::Product { fields } = representation else {
                return None;
            };
            if !fields.contains(&ValueShape::State) {
                return None;
            }
            let id = cc::ReprId(id as u32);
            let signature = cc::Signature {
                parameters: vec![ValueShape::State],
                result: ValueShape::Reference(cc::Reference {
                    nullable: false,
                    heap: RefShape::Repr(id),
                }),
            };
            cc::state::StateCallProjection::checked(&signature, &logical.representations)
                .ok()
                .flatten()
                .map(|plan| (id, plan))
        })
        .collect::<HashMap<_, _>>();
    let shape = |value: ValueShape| match value {
        ValueShape::Reference(cc::Reference {
            nullable: false,
            heap: RefShape::Repr(id),
        }) => steps.get(&id).map_or(value, |plan| plan.payload),
        _ => value,
    };
    let reachable =
        crate::mir::reachable::ReachableHandles::from_module(&logical).map_err(|failure| {
            vec![BackendError::new(
                "P9 State projection",
                logical.span,
                failure.to_string(),
            )]
        })?;
    // Project signatures in the logical inventory. Physical layout planning
    // subsequently recomputes reachability from the projected module; unused
    // logical Step entries need neither deletion nor representation reindexing.
    for id in reachable.signatures {
        let signature = &mut physical.representations.signatures[id.0 as usize];
        *signature =
            cc::state::StateCallProjection::physical_signature(signature, &logical.representations)
                .map_err(|message| {
                    vec![BackendError::new(
                        "P9 State projection",
                        logical.span,
                        message,
                    )]
                })?;
    }
    // Reachability is computed over the logical module, so a state-stepping
    // signature that only a projected-away Step referred to can survive
    // without being reachable. Project every remaining one: a signature with a
    // checked Step call has no physical State form to keep.
    for index in 0..physical.representations.signatures.len() {
        let signature = &mut physical.representations.signatures[index];
        if !signature.parameters.contains(&ValueShape::State) {
            continue;
        }
        *signature =
            cc::state::StateCallProjection::physical_signature(signature, &logical.representations)
                .map_err(|message| {
                    vec![BackendError::new(
                        "P9 State projection",
                        logical.span,
                        message,
                    )]
                })?;
    }
    // Only checked state-stepping declarations may enter private physical
    // planning. A signature that projects to a checked Step call carries its
    // own proof, so a host binding is admitted on the same terms as a runtime
    // one. Actual raw calls still require result adaptation and MIR
    // correspondence.
    for external in &mut physical.externals {
        let Some(signature) = external.signature.clone() else {
            continue;
        };
        if !signature.parameters.contains(&ValueShape::State) {
            continue;
        }
        let step = cc::state::StateCallProjection::checked(&signature, &logical.representations)
            .map_err(|message| {
                vec![BackendError::invalid_ir(
                    "P9 runtime projection",
                    logical.span,
                    message,
                )]
            })?;
        if step.is_none() && !runtime_symbols.contains(&external.symbol) {
            return Err(vec![BackendError::new(
                "P9 State projection",
                logical.span,
                "state-aware host invocation requires checked ABI projection",
            )]);
        }
        external.signature = Some(
            cc::state::StateCallProjection::physical_signature(
                &signature,
                &logical.representations,
            )
            .map_err(|message| {
                vec![BackendError::invalid_ir(
                    "P9 runtime projection",
                    logical.span,
                    message,
                )]
            })?,
        );
    }
    for function in &mut physical.functions {
        let original = logical
            .functions
            .iter()
            .find(|source| source.symbol == function.symbol)
            .unwrap();
        let states = original
            .values
            .iter()
            .filter(|value| value.ty == ValueShape::State)
            .map(|value| value.id)
            .collect::<HashSet<_>>();
        let has_step = original
            .values
            .iter()
            .any(|value| shape(value.ty) != value.ty);
        if has_step && !inventory.requires(function.symbol) {
            return Err(error(
                original,
                "stored Step requires checked dependency transport",
            ));
        }
        function.parameters.retain(|id| !states.contains(id));
        function.values.retain(|value| !states.contains(&value.id));
        for value in &mut function.values {
            value.ty = shape(value.ty);
        }
        function.result_type = shape(function.result_type);
        project_assignments(
            &mut function.assignments,
            original,
            &states,
            &steps,
            &shape,
            &logical.representations,
        )?;
    }

    Ok(ProjectedInput {
        physical,
        logical,
        inventory,
    })
}

fn error(function: &cc::Function, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::new("P9 State projection", function.span, message)
            .with_module(function.symbol.module),
    ]
}

fn project_assignments(
    input: &mut Vec<cc::Assignment>,
    original: &cc::Function,
    states: &HashSet<cc::ValueId>,
    steps: &HashMap<cc::ReprId, cc::state::StateCallProjection>,
    shape: &impl Fn(ValueShape) -> ValueShape,
    table: &cc::RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    let mut assignments = Vec::new();
    for mut assignment in std::mem::take(input) {
        if states.contains(&assignment.destination) {
            if matches!(assignment.kind, A::If { .. } | A::TagSwitch { .. }) {
                return Err(error(
                    original,
                    "raw State choice requires zero-width control result projection",
                ));
            }
            continue;
        }
        let copy = match &mut assignment.kind {
            A::ProductNew {
                representation,
                arguments,
                ..
            } => steps
                .get(representation)
                .map(|plan| (arguments[plan.payload_field], plan.payload)),
            A::ProductGet {
                representation,
                value,
                field,
                ..
            } => {
                if let Some(plan) = steps.get(representation) {
                    if *field as usize != plan.payload_field {
                        return Err(error(
                            original,
                            "State field has an unexpected physical destination",
                        ));
                    }
                    Some((*value, plan.payload))
                } else {
                    None
                }
            }
            A::StateExecution {
                function,
                signature,
                ..
            } => {
                assignment.kind = A::IndirectCall {
                    function: *function,
                    signature: *signature,
                    arguments: Vec::new(),
                };
                None
            }
            A::DirectCall { arguments, .. } | A::IndirectCall { arguments, .. } => {
                arguments.retain(|id| !states.contains(id));
                None
            }
            A::AggregateConvert { conversion, .. } => {
                let source = shape(conversion.source);
                let destination = shape(conversion.destination);
                if source != conversion.source || destination != conversion.destination {
                    *conversion =
                        cc::state::StateCallProjection::payload_conversion(conversion, table)
                            .map_err(|message| error(original, message))?;
                }
                conversion.source = source;
                conversion.destination = destination;
                None
            }
            A::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    project_assignments(
                        &mut case.assignments,
                        original,
                        states,
                        steps,
                        shape,
                        table,
                    )?;
                }
                project_assignments(default_assignments, original, states, steps, shape, table)?;
                None
            }
            A::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                project_assignments(then_assignments, original, states, steps, shape, table)?;
                project_assignments(else_assignments, original, states, steps, shape, table)?;
                None
            }
            _ => None,
        };
        if let A::AggregateConvert {
            value, conversion, ..
        } = &assignment.kind
            && let ValueConversion::FunctionAdapter { function, .. } = conversion.plan
        {
            // The checked factory signature already owns this conversion.
            // Keep the source destination as the call result, rather than an
            // unanchored helper temporary followed by a copy.
            assignment.kind = A::DirectCall {
                function,
                arguments: vec![*value],
            };
        }
        if let Some((value, payload)) = copy {
            assignment.kind = A::AggregateConvert {
                destination: assignment.destination,
                value,
                conversion: cc::AggregateConvert {
                    source: payload,
                    destination: payload,
                    plan: ValueConversion::Identity,
                },
            };
        }
        assignments.push(assignment);
    }
    *input = assignments;
    Ok(())
}
