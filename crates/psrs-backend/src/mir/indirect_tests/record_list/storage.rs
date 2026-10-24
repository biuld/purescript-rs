//! Canonical lists whose source Array stores records through erased slots.
use super::*;
use crate::cc::payload::{PayloadPlanner, StoragePayloadPlanner};
use crate::cc::{
    self, Assignment, AssignmentKind, ExternalProjection, Representation, ValueDecl, ValueShape,
};
use crate::types::ValueId;

pub(in crate::mir::indirect_tests) fn erased_array(mut module: cc::Module) -> cc::Module {
    let external = &module.externals[0];
    let signature = external.signature.as_ref().unwrap();
    let result = signature.parameters.is_empty();
    let array = if result {
        signature.result
    } else {
        signature.parameters[0]
    };
    let mut guest = cc::guest_layout(array, &module.representations).unwrap();
    let GuestLayout::Array { repr, element } = &mut guest else {
        panic!("array fixture")
    };
    let stored = element.stored;
    let array_repr = *repr;
    element.stored = cc::payload::erased_shape();
    module.representations.set(
        array_repr,
        Representation::Array {
            element: cc::payload::erased_shape(),
        },
    );
    module.representations.ensure_record_storage();
    for value in [ValueShape::Integer, ValueShape::Number] {
        let id = module.representations.reserve();
        module
            .representations
            .set(id, Representation::Box { value });
    }
    module.externals[0].projection = Some(if result {
        ExternalProjection {
            parameters: Vec::new(),
            result: Some(guest),
        }
    } else {
        ExternalProjection {
            parameters: vec![guest.clone()],
            result: (signature.result == array).then_some(guest),
        }
    });
    if !result {
        let plan = StoragePayloadPlanner(&module.representations)
            .erase_payload(stored, module.span)
            .unwrap();
        let function = &mut module.functions[0];
        let original = std::mem::take(&mut function.assignments);
        let mut next = function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap_or(0)
            + 1;
        for mut assignment in original {
            if let AssignmentKind::ArrayNew {
                representation,
                elements,
                ..
            } = &mut assignment.kind
                && *representation == array_repr
            {
                for element in elements {
                    let destination = ValueId(next);
                    next += 1;
                    function.values.push(ValueDecl {
                        id: destination,
                        ty: cc::payload::erased_shape(),
                    });
                    function.assignments.push(Assignment {
                        destination,
                        kind: AssignmentKind::AggregateConvert {
                            destination,
                            value: *element,
                            conversion: cc::AggregateConvert {
                                source: stored,
                                destination: cc::payload::erased_shape(),
                                plan: plan.clone(),
                            },
                        },
                        span: assignment.span,
                    });
                    *element = destination;
                }
            }
            function.assignments.push(assignment);
        }
    }
    module
}

fn lower(
    input: (cc::Module, crate::ExternalBindings, wit_parser::Resolve),
) -> (crate::mir::Module, abi::WasiRegistry, TargetCapabilities) {
    let (module, bindings, resolve) = input;
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, registry) =
        lower_module_with_registry(erased_array(module), bindings, target, registry)
            .expect("erased record list ABI should lower");
    crate::mir::verify_module(&mir).expect("the record storage owner and physical layout agree");
    (mir, registry, target)
}

#[test]
fn erased_record_lists_encode_both_abi_directions_and_string_fields() {
    for fixture in [
        fixture(),
        result_fixture(),
        string_fixture(),
        flags_fixture(),
    ] {
        let (mir, mut registry, target) = lower(fixture);
        let mir = crate::mir::opt::optimize(mir, target).expect("record list MIR optimization");
        let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
            .expect("record storage ABI encoding");
        let binary = crate::wasm::encode_module(&wasm).expect("record list binary");
        crate::validator_for(target)
            .validate_all(&binary)
            .expect("both directions validate with erased record storage");
    }
}

#[test]
fn mir_record_storage_views_require_the_owner_and_exact_gc_schema() {
    let (baseline, _, _) = lower(result_fixture());
    let layout = baseline.layout.as_ref().unwrap();
    let storage = layout.representation_table().record_storage().unwrap();
    let header = layout.repr_index(storage.representation()).unwrap();
    let keys = layout.repr_index(storage.labels()).unwrap();
    let mutations: &[fn(
        &mut crate::mir::Module,
        crate::types::DefinedTypeId,
        crate::types::DefinedTypeId,
    )] = &[
        |module, _, _| module.layout = None,
        |module, header, _| {
            module
                .types
                .iter_mut()
                .flat_map(|group| &mut group.0)
                .nth(header.0 as usize)
                .unwrap()
                .composite = crate::types::CompositeType::Struct(Vec::new())
        },
        |module, _, keys| {
            module
                .types
                .iter_mut()
                .flat_map(|group| &mut group.0)
                .nth(keys.0 as usize)
                .unwrap()
                .composite = crate::types::CompositeType::Array(crate::types::FieldType {
                storage: crate::types::StorageType::I32,
                mutable: true,
            })
        },
        |module, _, _| {
            for instruction in module
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.blocks)
                .flat_map(|block| &mut block.instructions)
            {
                if let Instruction::ListCopy {
                    element_guest: GuestLayout::RecordStorage { labels, .. },
                    ..
                } = instruction
                {
                    labels.swap(0, 1);
                }
            }
        },
        |module, _, _| {
            for instruction in module
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.blocks)
                .flat_map(|block| &mut block.instructions)
            {
                if let Instruction::ListCopy { struct_type, .. } = instruction {
                    *struct_type = crate::types::DefinedTypeId(u32::MAX);
                }
            }
        },
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut module = baseline.clone();
        mutate(&mut module, header, keys);
        let errors = crate::mir::verify_module(&module)
            .expect_err("malformed record storage evidence must fail");
        assert!(
            errors.iter().any(|error| error.message.contains("record")),
            "mutation {index}: {errors:?}"
        );
    }
}
