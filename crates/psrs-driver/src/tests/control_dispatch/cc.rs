//! CC dispatch negatives and provenance positives.
use super::*;

#[test]
fn cc_rejects_a_projection_of_the_wrong_case() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let carriers = [
        carrier_repr(table, R::Executor, 2),
        carrier_repr(table, R::Executor, 5),
        carrier_repr(table, R::CloneSequence, 0),
    ];
    for role in [R::Executor, R::CloneSequence, R::RebuildSequence] {
        let (symbol, _) = binding(table, role);
        let mut broken = input.cc.clone();
        let function = broken
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        let shapes = function
            .values
            .iter()
            .map(|value| (value.id, value.ty))
            .collect::<std::collections::HashMap<_, _>>();
        let changed = visit(&mut function.assignments, &mut |assignment| {
            let A::VariantGet {
                representation,
                case,
                field,
                ..
            } = &mut assignment.kind
            else {
                return false;
            };
            if !carriers.contains(representation) {
                return false;
            }
            let Some(Representation::Variant { cases }) = table.representation(*representation)
            else {
                return false;
            };
            let destination = shapes.get(&assignment.destination).copied();
            let Some(alternative) = cases.iter().find(|candidate| {
                candidate.tag != *case
                    && candidate.fields.get(*field as usize).copied() == destination
            }) else {
                return false;
            };
            *case = alternative.tag;
            true
        });
        assert!(changed, "{role:?}: no carrier projection to mutate");
        let errors = psrs_backend::cc::verify_module(&broken).expect_err("wrong projection");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("control projection must match")),
            "{role:?}: {errors:?}"
        );
    }
}

#[test]
fn cc_dispatch_requires_a_trapping_default() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let (symbol, _) = binding(table, R::Executor);
    let mut broken = input.cc.clone();
    let function = broken
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let state = function.parameters[0];
    let Some(A::TagSwitch { default_value, .. }) = function
        .assignments
        .last_mut()
        .map(|assignment| &mut assignment.kind)
    else {
        panic!("input dispatch");
    };
    // Point the default at an available value with the right shape so only the
    // trap requirement is violated.
    *default_value = state;
    let errors = psrs_backend::cc::verify_module(&broken).expect_err("non-trapping default");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("must trap on unsupported alternatives")),
        "{errors:?}"
    );
}

#[test]
fn cc_dispatch_resolves_a_reference_alias_of_the_input() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let (symbol, _) = binding(table, R::Executor);
    let mut broken = input.cc.clone();
    let function = broken
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let source = function.parameters[2];
    let declaration = function
        .values
        .iter()
        .find(|value| value.id == source)
        .unwrap()
        .clone();
    let alias = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(ValueDecl {
        id: alias,
        ty: declaration.ty,
    });
    let tag = match &function.assignments.last().unwrap().kind {
        A::TagSwitch { value, .. } => *value,
        _ => panic!("input dispatch"),
    };
    let position = function
        .assignments
        .iter()
        .position(|assignment| assignment.destination == tag)
        .unwrap();
    function.assignments.insert(
        position,
        Assignment {
            destination: alias,
            kind: A::AggregateConvert {
                destination: alias,
                value: source,
                conversion: AggregateConvert {
                    source: declaration.ty,
                    destination: declaration.ty,
                    plan: ValueConversion::Identity,
                },
            },
            span: function.span,
        },
    );
    let variant = function
        .assignments
        .iter_mut()
        .find(|assignment| assignment.destination == tag)
        .unwrap();
    let A::VariantTag { value, .. } = &mut variant.kind else {
        panic!("input tag");
    };
    *value = alias;
    psrs_backend::cc::verify_module(&broken)
        .expect("an identity alias must preserve dispatch provenance");
}

#[test]
fn cc_dispatch_result_must_originate_from_the_input() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let (symbol, _) = binding(table, R::Executor);
    let mut broken = input.cc.clone();
    let function = broken
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    // A result that is not the input dispatch stands in for a dead transition.
    function.result = function.parameters[0];
    let errors = psrs_backend::cc::verify_module(&broken).expect_err("dead dispatch");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("must be its current input dispatch")),
        "{errors:?}"
    );
}
