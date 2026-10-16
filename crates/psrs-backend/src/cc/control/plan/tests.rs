use super::*;
use crate::cc::{Representation, Signature, ValueShape};

fn fixture() -> (
    RepresentationTable,
    Vec<ControlHelperContract>,
    ControlResumptionContract,
    ControlSchema,
) {
    let mut table = RepresentationTable::default();
    let payload = crate::cc::payload::erased_shape();
    let continuation = table.add_signature(Signature {
        parameters: vec![payload],
        result: payload,
    });
    let nominal = crate::cc::layout::control::reserve(&mut table);
    let layout = crate::cc::layout::control::append(&mut table, continuation, &nominal);
    let schema = ControlSchema {
        computation: layout.computation,
        sequence: layout.sequence,
        context: layout.context,
        extent: layout.boundary,
        status: layout.status,
        owner: layout.owner,
        callback: layout.callback,
        result_pair: layout.pair,
        replay_result: layout.replay_result,
    };
    let helpers = ControlHelperRole::ALL
        .into_iter()
        .enumerate()
        .map(|(index, role)| {
            let signature = match role {
                ControlHelperRole::Executor => layout.executor,
                ControlHelperRole::Run => layout.run,
                ControlHelperRole::CloneSequence => layout.clone_sequence,
                ControlHelperRole::RebuildSequence => layout.rebuild_sequence,
                ControlHelperRole::Start => layout.start,
                ControlHelperRole::Check | ControlHelperRole::Close => layout.boundary_check,
                ControlHelperRole::Once => layout.once,
                ControlHelperRole::Cancel => layout.cancel,
            };
            ControlHelperContract {
                role,
                signature,
                function: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), index as u32),
            }
        })
        .collect();
    let raw = ControlResumptionContract {
        function: psrs_hir::SymbolId::new(
            psrs_hir::ModuleId(0),
            ControlHelperRole::ALL.len() as u32,
        ),
        signature: continuation,
    };
    (table, helpers, raw, schema)
}

#[test]
fn freezing_is_single_assignment_and_preserves_the_published_plan() {
    let (mut table, helpers, raw, schema) = fixture();
    table
        .freeze_control_plan(helpers.clone(), raw.clone(), schema)
        .unwrap();
    let frozen = table.control_plan().unwrap().clone();
    assert_eq!(
        frozen.helper(ControlHelperRole::Executor),
        Some(&helpers[0])
    );
    let error = table.freeze_control_plan(helpers, raw, schema).unwrap_err();
    assert_eq!(error, "control machine plan is already frozen");
    assert_eq!(table.control_plan(), Some(&frozen));
}

#[test]
fn planning_failure_does_not_publish_a_partial_plan_and_can_be_retried() {
    let (mut table, helpers, raw, schema) = fixture();
    let identity = crate::cc::TypeIdentity::Constructor(crate::cc::TypeConstructorIdentity::Int);
    let id = table.plan_control_identity(identity.clone());
    let original = table.representation(schema.status).unwrap().clone();
    table.set(
        schema.status,
        Representation::Array {
            element: ValueShape::Boolean,
        },
    );
    assert!(
        table
            .freeze_control_plan(helpers.clone(), raw.clone(), schema)
            .is_err()
    );
    assert!(table.control_plan().is_none());
    assert_eq!(
        table.source_type_identities().get(id.0 as usize),
        Some(&identity)
    );
    table.set(schema.status, original);
    table.freeze_control_plan(helpers, raw, schema).unwrap();
    assert_eq!(
        table
            .control_plan()
            .unwrap()
            .source_type_identities()
            .get(id.0 as usize),
        Some(&identity)
    );
}

#[test]
fn helper_roles_and_symbols_must_be_complete_and_unique_before_publication() {
    for mutation in 0..3 {
        let (mut table, mut helpers, raw, schema) = fixture();
        match mutation {
            0 => {
                helpers.pop();
            }
            1 => helpers[1].role = helpers[0].role,
            2 => helpers[1].function = raw.function(),
            _ => unreachable!(),
        }
        assert!(table.freeze_control_plan(helpers, raw, schema).is_err());
        assert!(table.control_plan().is_none());
    }
}

#[test]
fn same_arity_layout_mutations_cannot_change_machine_storage_roles() {
    for mutation in 0..4 {
        let (mut table, helpers, raw, schema) = fixture();
        table.freeze_control_plan(helpers, raw, schema).unwrap();
        match mutation {
            0 => {
                let Representation::Variant { mut cases } =
                    table.representation(schema.sequence).unwrap().clone()
                else {
                    unreachable!()
                };
                cases[1].fields.swap(0, 1);
                table.set(schema.sequence, Representation::Variant { cases });
            }
            1 => table.set(
                schema.status,
                Representation::Array {
                    element: ValueShape::Boolean,
                },
            ),
            2 => {
                let Representation::Product { mut fields } =
                    table.representation(schema.replay_result).unwrap().clone()
                else {
                    unreachable!()
                };
                fields.swap(0, 1);
                table.set(schema.replay_result, Representation::Product { fields });
            }
            3 => {
                let signature = table
                    .control_plan()
                    .unwrap()
                    .helper(ControlHelperRole::Executor)
                    .unwrap()
                    .signature();
                table.signatures[signature.0 as usize].parameters.swap(2, 3);
            }
            _ => unreachable!(),
        }
        assert!(
            table.verify_control_schema().is_err(),
            "mutation {mutation} escaped schema validation"
        );
    }
}
