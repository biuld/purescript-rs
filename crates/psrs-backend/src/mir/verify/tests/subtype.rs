//! Defined-type subtyping fixtures: mutability, finality, and nullability.

use super::verify_module;
use crate::mir::Module;
use crate::types::{
    CompositeType, DefinedType, DefinedTypeId, FieldType, HeapType, RecGroup, RefType, StorageType,
    ValueType,
};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module_with(types: Vec<RecGroup>) -> Module {
    Module {
        name: "SubtypeTest".into(),
        types,
        imports: Vec::new(),
        functions: Vec::new(),
        entry: None,
        span: span(),
    }
}

fn field(storage: StorageType, mutable: bool) -> FieldType {
    FieldType { storage, mutable }
}

fn struct_type(
    final_type: bool,
    supertype: Option<DefinedTypeId>,
    fields: Vec<FieldType>,
) -> DefinedType {
    DefinedType {
        final_type,
        supertype,
        composite: CompositeType::Struct(fields),
    }
}

fn func_type(
    final_type: bool,
    supertype: Option<DefinedTypeId>,
    parameters: Vec<ValueType>,
    results: Vec<ValueType>,
) -> DefinedType {
    DefinedType {
        final_type,
        supertype,
        composite: CompositeType::Func {
            parameters,
            results,
        },
    }
}

fn eq_ref(nullable: bool) -> ValueType {
    ValueType::Ref(RefType {
        nullable,
        heap: HeapType::Eq,
    })
}

#[test]
fn accepts_a_valid_struct_case_subtype() {
    let module = module_with(vec![RecGroup(vec![
        struct_type(false, None, vec![field(StorageType::I32, false)]),
        struct_type(
            true,
            Some(DefinedTypeId(0)),
            vec![
                field(StorageType::I32, false),
                field(StorageType::I32, false),
            ],
        ),
    ])]);
    verify_module(&module).expect("a case subtype of a non-final supertype must verify");
}

#[test]
fn rejects_a_final_declared_supertype() {
    let module = module_with(vec![RecGroup(vec![
        struct_type(true, None, vec![field(StorageType::I32, false)]),
        struct_type(
            true,
            Some(DefinedTypeId(0)),
            vec![
                field(StorageType::I32, false),
                field(StorageType::I32, false),
            ],
        ),
    ])]);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("earlier non-final")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_subtype_that_changes_field_mutability() {
    let module = module_with(vec![RecGroup(vec![
        struct_type(false, None, vec![field(StorageType::I32, false)]),
        struct_type(
            true,
            Some(DefinedTypeId(0)),
            vec![field(StorageType::I32, true)],
        ),
    ])]);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("incompatible with its declared supertype")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_subtype_that_widens_field_nullability() {
    let module = module_with(vec![RecGroup(vec![
        struct_type(
            false,
            None,
            vec![field(
                StorageType::Ref(RefType {
                    nullable: false,
                    heap: HeapType::Eq,
                }),
                false,
            )],
        ),
        struct_type(
            true,
            Some(DefinedTypeId(0)),
            vec![
                field(
                    StorageType::Ref(RefType {
                        nullable: true,
                        heap: HeapType::Eq,
                    }),
                    false,
                ),
                field(StorageType::I32, false),
            ],
        ),
    ])]);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("incompatible with its declared supertype")),
        "{errors:?}"
    );
}

#[test]
fn accepted_value_types_are_well_formed() {
    // A sanity check that an abstract `eqref` field survives verification.
    let module = module_with(vec![RecGroup(vec![struct_type(
        true,
        None,
        vec![field(
            StorageType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }),
            false,
        )],
    )])]);
    verify_module(&module).expect("an abstract eqref field must verify");
}

#[test]
fn accepts_contravariant_parameters_and_covariant_results() {
    let module = module_with(vec![RecGroup(vec![
        func_type(false, None, vec![eq_ref(false)], vec![eq_ref(true)]),
        func_type(
            true,
            Some(DefinedTypeId(0)),
            vec![eq_ref(true)],
            vec![eq_ref(false)],
        ),
    ])]);
    verify_module(&module).expect("a function subtype must respect variance");
}

#[test]
fn rejects_a_function_subtype_that_widens_its_result() {
    let module = module_with(vec![RecGroup(vec![
        func_type(false, None, vec![eq_ref(true)], vec![eq_ref(false)]),
        func_type(
            true,
            Some(DefinedTypeId(0)),
            vec![eq_ref(true)],
            vec![eq_ref(true)],
        ),
    ])]);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("incompatible with its declared supertype")),
        "{errors:?}"
    );
}
