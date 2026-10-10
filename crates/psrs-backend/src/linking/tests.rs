use super::*;
use crate::types::{
    CompositeType, DefinedType, DefinedTypeId, FieldType, HeapType, RecGroup, RefType, StorageType,
    ValueType,
};

fn module(import: mir::Import) -> mir::Module {
    mir::Module {
        name: "ConsumerAbi".into(),
        entry: None,
        types: vec![],
        strings: vec![],
        imports: vec![import],
        functions: vec![],
        dependencies: Default::default(),
        layout: None,
        span: TextRange::new(0, 1),
    }
}

fn plans(module: &mir::Module) -> bool {
    plan_for_module(
        &default_context().unwrap(),
        module,
        &mut WasiRegistry::load().unwrap(),
        TargetCapabilities::default(),
    )
    .is_ok()
}

#[test]
fn actual_artifact_consumer_signatures_are_checked_before_emission() {
    for provider in crate::target_intrinsics::artifacts() {
        assert!(plans(&module(provider.import())));
        let good = provider.import();
        let mut invalid = Vec::new();
        if good.parameters.is_empty() {
            // A numeric constant has no parameter to retarget. An added argument is
            // the arity mismatch that a unary export expresses by dropping one.
            let mut wrong = good.clone();
            wrong.parameters.push(ValueType::F64);
            invalid.push(wrong);
        } else {
            let mut wrong = good.clone();
            wrong.parameters[0] = if wrong.parameters[0] == ValueType::I32 {
                ValueType::F64
            } else {
                ValueType::I32
            };
            invalid.push(wrong);
            let mut wrong = good.clone();
            wrong.parameters.pop();
            invalid.push(wrong);
            let mut wrong = good.clone();
            wrong.parameters[0] = ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Any,
            });
            invalid.push(wrong);
        }
        let mut wrong = good.clone();
        wrong.result = None;
        invalid.push(wrong);
        let mut wrong = good;
        wrong.result = Some(ValueType::I64);
        invalid.push(wrong);
        for import in invalid {
            let module = module(import);
            // MIR's local call contract can be consistent while its provider ABI is wrong.
            crate::mir::verify_module(&module).unwrap();
            assert!(!plans(&module));
            assert!(
                crate::wasm::lower_module(&module, &mut WasiRegistry::load().unwrap()).is_err()
            );
        }
    }
}

#[test]
fn generated_bindings_match_the_shared_body_signature_and_string_layout() {
    let string = Some(DefinedTypeId(0));
    for symbol in [
        abi::REALLOC_SYMBOL,
        abi::STRING_TO_BYTES_SYMBOL,
        abi::BYTES_TO_STRING_SYMBOL,
        abi::VALIDATE_STEP_SYMBOL,
    ] {
        let mut good =
            module(crate::target_intrinsics::generated::signature(symbol, string).unwrap());
        good.types.push(RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Array(FieldType {
                storage: StorageType::I8,
                mutable: true,
            }),
        }]));
        // validate_step is synthesized together with codecs and requires their string type.
        if symbol == abi::VALIDATE_STEP_SYMBOL {
            good.imports.push(
                crate::target_intrinsics::generated::signature(abi::STRING_TO_BYTES_SYMBOL, string)
                    .unwrap(),
            );
        }
        assert!(plans(&good), "{symbol:?}");
        let mut wrong = good.clone();
        wrong.imports[0].result = None;
        assert!(!plans(&wrong));
        let mut wrong = good.clone();
        wrong.imports[0].parameters.push(ValueType::I32);
        assert!(!plans(&wrong));
        if symbol == abi::STRING_TO_BYTES_SYMBOL || symbol == abi::BYTES_TO_STRING_SYMBOL {
            let mut wrong = good.clone();
            wrong.types[0].0[0].composite = CompositeType::Array(FieldType {
                storage: StorageType::I32,
                mutable: true,
            });
            assert!(!plans(&wrong));
            let mut wrong = good;
            let ty = if symbol == abi::STRING_TO_BYTES_SYMBOL {
                &mut wrong.imports[0].parameters[0]
            } else {
                wrong.imports[0].result.as_mut().unwrap()
            };
            *ty = ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Index(DefinedTypeId(0)),
            });
            assert!(!plans(&wrong));
        }
    }
}
