use super::*;
use crate::cc::{External, ReprId, RepresentationTable, Signature};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn reference(id: u32) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(ReprId(id)),
    })
}

fn fixture(operation: StorageOperation) -> (RuntimeBinding, cc::Module) {
    let span = TextRange::new(0, 1);
    let binding = RuntimeBinding {
        symbol: SymbolId::new(ModuleId(1), 2),
        source_module: ModuleId(1),
        module: psrs_runtime::STORAGE_MODULE.into(),
        function: operation.abi().export.into(),
        type_id: Some(psrs_core::TypeId(0)),
        span,
    };
    let shape = |role| match role {
        StorageValue::Int | StorageValue::Unit => ValueShape::Integer,
        StorageValue::Array => reference(0),
        StorageValue::Element | StorageValue::Any => cc::payload::erased_shape(),
    };
    let source = operation.source_contract();
    let mut parameters = source
        .parameters
        .iter()
        .copied()
        .map(shape)
        .collect::<Vec<_>>();
    parameters.push(ValueShape::State);
    let module = cc::Module {
        name: "RuntimeBoundary".into(),
        externals: vec![External {
            symbol: binding.symbol,
            signature: Some(Signature {
                parameters,
                result: reference(1),
            }),
            projection: None,
        }],
        representations: RepresentationTable {
            representations: vec![
                Representation::Array {
                    element: cc::payload::erased_shape(),
                },
                Representation::Product {
                    fields: vec![shape(source.payload), ValueShape::State],
                },
            ],
            ..Default::default()
        },
        functions: Vec::new(),
        entry: None,
        span,
    };
    (binding, module)
}

#[test]
fn monomorphic_elements_use_the_payload_protocol_without_changing_other_roles() {
    for operation in StorageOperation::ALL {
        let (binding, mut module) = fixture(operation);
        let source = operation.source_contract();
        let signature = module.externals[0].signature.as_mut().unwrap();
        for (shape, role) in signature.parameters.iter_mut().zip(source.parameters) {
            if *role == StorageValue::Element {
                *shape = ValueShape::Integer;
            }
        }
        if source.payload == StorageValue::Element {
            module.representations.set(
                ReprId(1),
                Representation::Product {
                    fields: vec![ValueShape::Integer, ValueShape::State],
                },
            );
        }
        let original = signature.clone();
        module.representations.add_signature(original.clone());
        cc::state::register_payload_slots(&mut module.representations);
        let canonical = canonical_signature(&binding, &original, &module.representations).unwrap();
        for ((before, after), role) in original
            .parameters
            .iter()
            .zip(&canonical.parameters)
            .zip(source.parameters)
        {
            assert_eq!(
                *after,
                if *role == StorageValue::Element {
                    cc::payload::erased_shape()
                } else {
                    *before
                }
            );
        }
        assert_eq!(canonical.parameters.last(), Some(&ValueShape::State));
        let step = cc::state::StateCallProjection::checked(&canonical, &module.representations)
            .unwrap()
            .unwrap();
        assert_eq!(step.state_field, 1);
        if source.payload == StorageValue::Element {
            assert_eq!(step.payload, cc::payload::erased_shape());
        } else {
            assert_eq!(canonical.result, original.result);
        }
    }
}

#[test]
fn validates_all_raw_contracts_with_permuted_step_fields() {
    for operation in StorageOperation::ALL {
        let (binding, module) = fixture(operation);
        let plan = checked(&binding, &module).unwrap();
        assert_eq!(plan.abi().export, operation.abi().export);
        assert_eq!(plan.result(), operation.projection().unwrap().result());
    }
}

#[test]
fn rejects_noncanonical_array_storage_before_raw_call_projection() {
    for operation in [
        StorageOperation::Fill,
        StorageOperation::Read,
        StorageOperation::Write,
    ] {
        let (binding, mut module) = fixture(operation);
        for representation in [
            Representation::Array {
                element: ValueShape::Integer,
            },
            Representation::Product {
                fields: vec![cc::payload::erased_shape()],
            },
        ] {
            module.representations.set(ReprId(0), representation);
            let errors = checked(&binding, &module).err().unwrap();
            assert!(
                errors[0]
                    .message
                    .contains("canonical erased-element storage")
            );
        }
    }
}

#[test]
fn rejects_logical_signature_and_provider_substitution() {
    for mutation in 0..8 {
        let (mut binding, mut module) = fixture(StorageOperation::Read);
        let signature = module.externals[0].signature.as_mut().unwrap();
        match mutation {
            0 => binding.module = "other-runtime".into(),
            1 => binding.function = "other-export".into(),
            2 => signature.parameters.pop().map(|_| ()).unwrap(),
            3 => signature.parameters.insert(0, ValueShape::State),
            4 => signature.parameters.insert(0, ValueShape::Integer),
            5 => signature.parameters[1] = ValueShape::Boolean,
            6 => signature.result = cc::payload::erased_shape(),
            _ => {
                signature.parameters[0] = ValueShape::Reference(Reference {
                    nullable: true,
                    heap: RefShape::Repr(ReprId(0)),
                })
            }
        }
        assert!(checked(&binding, &module).is_err(), "mutation {mutation}");
    }
}

#[test]
fn rejects_payload_and_successor_layout_drift() {
    for (operation, fields) in [
        (
            StorageOperation::Fill,
            vec![ValueShape::Integer, ValueShape::State],
        ),
        (
            StorageOperation::Write,
            vec![cc::payload::erased_shape(), ValueShape::State],
        ),
        (
            StorageOperation::Read,
            vec![ValueShape::State, ValueShape::State],
        ),
        (StorageOperation::Read, vec![cc::payload::erased_shape()]),
    ] {
        let (binding, mut module) = fixture(operation);
        module
            .representations
            .set(ReprId(1), Representation::Product { fields });
        assert!(checked(&binding, &module).is_err());
    }
}
