use crate::{
    BindingRequirement, Boundary, MemoryDemand, Provider, RequirementId, TargetLinkInput,
    TargetPolicy,
};

#[test]
fn gc_application_contracts_compare_closed_types_before_composition() {
    let context = crate::resolve_default_definitions().unwrap();
    let units = crate::runtime::package_offers(&psrs_runtime::PSRS_RUNTIME).unwrap();
    let storage = units
        .iter()
        .find(|unit| unit.id == psrs_runtime::STORAGE_UNIT.id)
        .unwrap();
    let read = storage
        .provided
        .iter()
        .find(|op| op.export == "array_read")
        .unwrap();
    let plan = crate::plan(
        &context,
        TargetLinkInput {
            requirements: vec![BindingRequirement {
                id: RequirementId(0),
                origin: "storage read".into(),
                boundary: Boundary::RawCore {
                    module: psrs_runtime::STORAGE_MODULE.into(),
                    field: read.export.clone(),
                },
                expected: Some(read.signature.clone()),
                provider: Provider::RuntimeOperation {
                    name: read.name.clone(),
                    version: read.version.clone(),
                },
            }],
            units,
            policy: TargetPolicy::default(),
            memory: MemoryDemand {
                canonical_scratch: (0, 16),
                allocator_state: (16, 24),
                base_heap_start: 24,
                heap_alignment: 8,
                growth_owner: crate::GENERATED_GROWTH_OWNER.into(),
                maximum_pages: None,
            },
        },
    )
    .unwrap();
    let text = r#"(module
        (type (struct (field i64)))
        (type $a (array (mut eqref)))
        (import "psrs:runtime-storage" "array_read" (func (param (ref $a) i32) (result eqref)))
        (memory (export "memory") 2))"#;
    super::verify(&plan, &wat::parse_str(text).unwrap()).unwrap();
    let declaration = "(import \"psrs:runtime-storage\" \"array_read\" (func (param (ref $a) i32) (result eqref)))";
    let aliases = text.replace(declaration, &format!("{declaration}\n{declaration}"));
    let bytes = wat::parse_str(&aliases).unwrap();
    wasmparser::Validator::new().validate_all(&bytes).unwrap();
    super::verify(&plan, &bytes).unwrap();
    let wrong_alias = text.replace(
        declaration,
        &format!(
            "{declaration}\n{}",
            declaration.replace("(ref $a)", "(ref null $a)")
        ),
    );
    let error = super::verify(&plan, &wat::parse_str(wrong_alias).unwrap()).unwrap_err();
    assert!(error.to_string().contains("application import signature"));
    for modified in [
        text.replace("(ref $a)", "(ref null $a)"),
        text.replace("(mut eqref)", "eqref"),
    ] {
        let error = super::verify(&plan, &wat::parse_str(modified).unwrap()).unwrap_err();
        assert!(
            error.to_string().contains("application import signature"),
            "{error}"
        );
    }
}
