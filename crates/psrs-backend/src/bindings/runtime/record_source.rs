//! Source-level record provider validation before row and payload erasure.
use psrs_core::{Module, Type, TypeConstructor, TypeId};
use psrs_hir::TypeId as HirTypeId;
use psrs_runtime::{RecordPrimitive, RecordSourceValue};

pub(super) fn validate(
    module: &Module,
    ty: TypeId,
    operation: RecordPrimitive,
) -> Result<(), String> {
    let (parameters, result) = psrs_core::call_parts(&module.types, ty).map_err(str::to_owned)?;
    let contract = operation.source_contract();
    if parameters.len() != contract.parameters.len() {
        return Err("record primitive signature has the wrong parameter count".into());
    }
    let mut record_type = None;
    let mut slot_type = None;
    for (actual, expected) in parameters
        .into_iter()
        .zip(contract.parameters)
        .chain(std::iter::once((result, &contract.result)))
    {
        match expected {
            RecordSourceValue::Int => require_constructor(module, actual, TypeConstructor::Int)?,
            RecordSourceValue::String => {
                require_constructor(module, actual, TypeConstructor::String)?
            }
            RecordSourceValue::Record => {
                if !module.is_record_type(actual) {
                    return Err("record primitive signature expects Record".into());
                }
                if record_type.is_some_and(|previous| !module.types_equivalent(previous, actual)) {
                    return Err("record primitive uses inconsistent Record rows".into());
                }
                record_type = Some(actual);
            }
            RecordSourceValue::Slot => {
                require_slot(module, actual)?;
                if slot_type.is_some_and(|previous| !module.types_equivalent(previous, actual)) {
                    return Err("record primitive uses inconsistent Slot types".into());
                }
                slot_type = Some(actual);
            }
            RecordSourceValue::ArrayString => {
                require_array_element(
                    module,
                    actual,
                    |element| {
                        module.types.get(element.0 as usize)
                            == Some(&Type::Constructor(TypeConstructor::String))
                    },
                    "Array String",
                )?;
            }
            RecordSourceValue::ArraySlot => {
                let element = require_array_element(
                    module,
                    actual,
                    |element| is_slot(module, element),
                    "Array Slot",
                )?;
                require_slot(module, element)?;
                if slot_type.is_some_and(|previous| !module.types_equivalent(previous, element)) {
                    return Err("record primitive uses inconsistent Slot types".into());
                }
                slot_type = Some(element);
            }
        }
    }
    Ok(())
}

fn require_constructor(
    module: &Module,
    id: TypeId,
    expected: TypeConstructor,
) -> Result<(), String> {
    if module.types.get(id.0 as usize) == Some(&Type::Constructor(expected)) {
        Ok(())
    } else {
        Err(format!("record primitive signature expects {expected:?}"))
    }
}

fn require_array_element(
    module: &Module,
    id: TypeId,
    valid: impl FnOnce(TypeId) -> bool,
    expected: &str,
) -> Result<TypeId, String> {
    let Some((TypeConstructor::Array, arguments)) = module.applied_constructor(id) else {
        return Err(format!("record primitive signature expects {expected}"));
    };
    let [element] = arguments.as_slice() else {
        return Err("record primitive Array must have one element type".into());
    };
    if !valid(*element) {
        return Err(format!("record primitive signature expects {expected}"));
    }
    Ok(*element)
}

fn require_slot(module: &Module, id: TypeId) -> Result<(), String> {
    if is_slot(module, id) {
        Ok(())
    } else {
        Err("record primitive Slot must be a declared constructorless ordinary data type".into())
    }
}

fn is_slot(module: &Module, id: TypeId) -> bool {
    let Some((TypeConstructor::User(type_id), _arguments)) = module.applied_constructor(id) else {
        return false;
    };
    is_ordinary_constructorless_data(module, type_id)
}

fn is_ordinary_constructorless_data(module: &Module, type_id: HirTypeId) -> bool {
    module.ordinary_data_ids.contains(&type_id)
        && !module.newtype_ids.contains(&type_id)
        && !module.opaque_ids.contains(&type_id)
        && !module
            .constructors
            .iter()
            .any(|constructor| constructor.type_id == type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_core::{ConstructorInfo, Type};
    use psrs_hir::{ModuleId, SymbolId, TypeVariableId};
    use psrs_span::TextRange;

    const SLOT_ID: HirTypeId = HirTypeId::new(ModuleId(7), 4);
    const FOREIGN_ID: HirTypeId = HirTypeId::new(ModuleId(7), 5);

    fn fixture() -> Module {
        let types = vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::String),
            Type::Constructor(TypeConstructor::User(SLOT_ID)),
            Type::RowEmpty,
            Type::Constructor(TypeConstructor::Record),
            Type::Application(TypeId(4), TypeId(3)),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(TypeId(6), TypeId(1)),
            Type::Application(TypeId(6), TypeId(2)),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(2), TypeId(0)),
            Type::Application(TypeId(6), TypeId(10)),
            Type::Variable(TypeVariableId(0)),
            Type::Constructor(TypeConstructor::User(FOREIGN_ID)),
        ];
        Module {
            type_names: vec![
                (SLOT_ID, "Carrier.Cell".into()),
                (FOREIGN_ID, "Carrier.Foreign".into()),
            ],
            id: ModuleId(1),
            name: "Carrier".into(),
            externals: Vec::new(),
            external_types: Vec::new(),
            types,
            newtype_ids: Vec::new(),
            ordinary_data_ids: vec![SLOT_ID],
            opaque_ids: Vec::new(),
            callable_types: Vec::new(),
            sources: Default::default(),
            constructors: Vec::new(),
            declarations: Vec::new(),
            entry: None,
            span: TextRange::new(0, 1),
        }
    }

    fn function(module: &mut Module, parameters: &[TypeId], result: TypeId) -> TypeId {
        let function = TypeId(9);
        let mut result = result;
        for parameter in parameters.iter().rev() {
            let partial = TypeId(module.types.len() as u32);
            module.types.push(Type::Application(function, *parameter));
            let arrow = TypeId(module.types.len() as u32);
            module.types.push(Type::Application(partial, result));
            result = arrow;
        }
        result
    }

    #[test]
    fn accepts_constructorless_declared_slot_and_concrete_record_build() {
        let mut module = fixture();
        let bare_payload = function(&mut module, &[TypeId(0), TypeId(5)], TypeId(2));
        validate(&module, bare_payload, RecordPrimitive::PayloadAt).unwrap();

        let payload = function(&mut module, &[TypeId(0), TypeId(5)], TypeId(10));
        validate(&module, payload, RecordPrimitive::PayloadAt).unwrap();
        let build = function(&mut module, &[TypeId(7), TypeId(11)], TypeId(5));
        validate(&module, build, RecordPrimitive::Build).unwrap();
    }

    #[test]
    fn rejects_missing_conflicting_and_nonconstructorless_slot_evidence() {
        let (base, payload) = payload_fixture();
        validate(&base, payload, RecordPrimitive::PayloadAt)
            .expect("the shared fixture uses a valid declared carrier");

        let mut missing = base.clone();
        missing.ordinary_data_ids.clear();
        assert_carrier_rejected(&missing, payload);

        let mut conflicting = base.clone();
        conflicting.opaque_ids.push(SLOT_ID);
        assert_carrier_rejected(&conflicting, payload);

        let mut newtype = base.clone();
        newtype.newtype_ids.push(SLOT_ID);
        assert_carrier_rejected(&newtype, payload);

        let mut constructor_bearing = base.clone();
        constructor_bearing.constructors.push(ConstructorInfo {
            symbol: SymbolId::new(ModuleId(7), 5),
            name: "Cell".into(),
            type_id: SLOT_ID,
            tag: 0,
            field_count: 0,
            field_types: Vec::new(),
            parameters: Vec::new(),
        });
        assert_carrier_rejected(&constructor_bearing, payload);

        let mut foreign = base.clone();
        foreign.opaque_ids.push(FOREIGN_ID);
        let foreign_payload = function(&mut foreign, &[TypeId(0), TypeId(5)], TypeId(13));
        assert_carrier_rejected(&foreign, foreign_payload);

        let mut variable = base.clone();
        let type_variable = function(&mut variable, &[TypeId(0), TypeId(5)], TypeId(12));
        assert_carrier_rejected(&variable, type_variable);
    }

    fn payload_fixture() -> (Module, TypeId) {
        let mut module = fixture();
        let payload = function(&mut module, &[TypeId(0), TypeId(5)], TypeId(10));
        (module, payload)
    }

    fn assert_carrier_rejected(module: &Module, payload: TypeId) {
        let error = validate(module, payload, RecordPrimitive::PayloadAt)
            .expect_err("malformed slot evidence must reject the checked source contract");
        assert_eq!(
            error,
            "record primitive Slot must be a declared constructorless ordinary data type"
        );
    }
}
