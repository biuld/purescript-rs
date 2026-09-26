//! Target-aware linking helpers for source WIT bindings.
//!
//! A foreign import's resolved source type is interned into the Core type table
//! so the backend can refer to it by identity. A structurally equal Core type
//! already in the table is reused, so a foreign signature shares the canonical
//! representation of the same type used elsewhere in the module. This replaces
//! recovering the type by structural search at the CC boundary.

use super::classification::source_enum_type;
use psrs_core::{
    ConstructorInfo, Module as CoreModule, Type as CoreType, TypeConstructor, TypeId as CoreTypeId,
};
use psrs_hir::{BuiltinType, Type as HirType, TypeKind as HirTypeKind};

/// Interns the resolved source type of a foreign import and returns its
/// [`CoreTypeId`]. The type is appended to the module type table when no
/// structurally equal type is present.
pub(crate) fn intern_source_type(module: &mut CoreModule, ty: &HirType) -> Option<CoreTypeId> {
    intern_into(&mut module.types, &module.constructors, ty)
}

fn intern_into(
    types: &mut Vec<CoreType>,
    constructors: &[ConstructorInfo],
    ty: &HirType,
) -> Option<CoreTypeId> {
    let core = match &ty.kind {
        HirTypeKind::Constructor(BuiltinType::Int) => CoreType::I32,
        HirTypeKind::Constructor(BuiltinType::Boolean) => CoreType::Boolean,
        HirTypeKind::Constructor(BuiltinType::Number) => CoreType::F64,
        HirTypeKind::Constructor(BuiltinType::Char) => CoreType::Char,
        HirTypeKind::Constructor(BuiltinType::String) => CoreType::String,
        HirTypeKind::Constructor(BuiltinType::Unit) => CoreType::Unit,
        HirTypeKind::Constructor(BuiltinType::Array) => {
            CoreType::Constructor(TypeConstructor::Array)
        }
        HirTypeKind::Opaque(type_id) => CoreType::Constructor(TypeConstructor::User(*type_id)),
        HirTypeKind::Named(type_id) => {
            source_enum_type(constructors, *type_id)?;
            CoreType::Constructor(TypeConstructor::User(*type_id))
        }
        HirTypeKind::Application(function, argument) => {
            let function = intern_into(types, constructors, function)?;
            let argument = intern_into(types, constructors, argument)?;
            if is_array_constructor(types, function) && !is_array_element(types, argument) {
                return None;
            }
            CoreType::Application(function, argument)
        }
        HirTypeKind::Function { parameter, result } => {
            let parameter = intern_into(types, constructors, parameter)?;
            let result = intern_into(types, constructors, result)?;
            CoreType::Function { parameter, result }
        }
        HirTypeKind::Record { fields, tail: None } => {
            let mut fields = fields
                .iter()
                .map(|field| {
                    let id = intern_into(types, constructors, &field.ty)?;
                    Some((field.label.clone(), id))
                })
                .collect::<Option<Vec<_>>>()?;
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            CoreType::Record(fields)
        }
        _ => return None,
    };
    Some(intern_core_type(types, core))
}

fn is_array_constructor(types: &[CoreType], id: CoreTypeId) -> bool {
    matches!(
        types.get(id.0 as usize),
        Some(CoreType::Constructor(TypeConstructor::Array))
    )
}

fn is_array_element(types: &[CoreType], id: CoreTypeId) -> bool {
    matches!(
        types.get(id.0 as usize),
        Some(CoreType::I32 | CoreType::Boolean | CoreType::F64 | CoreType::Char | CoreType::String)
    )
}

fn intern_core_type(types: &mut Vec<CoreType>, core: CoreType) -> CoreTypeId {
    if let Some(index) = types.iter().position(|existing| *existing == core) {
        return CoreTypeId(index as u32);
    }
    types.push(core);
    CoreTypeId((types.len() - 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::{ModuleId, TypeField};
    use psrs_span::TextRange;

    fn span() -> TextRange {
        TextRange::new(0, 1)
    }

    fn module() -> CoreModule {
        CoreModule {
            id: ModuleId(0),
            name: "Main".into(),
            externals: Vec::new(),
            types: vec![
                CoreType::I32,
                CoreType::F64,
                CoreType::Record(vec![
                    ("first".into(), CoreTypeId(0)),
                    ("secondValue".into(), CoreTypeId(1)),
                ]),
                CoreType::Unit,
            ],
            newtype_ids: Vec::new(),
            opaque_ids: Vec::new(),
            constructors: Vec::new(),
            declarations: Vec::new(),
            entry: None,
            span: span(),
        }
    }

    fn field(label: &str, kind: BuiltinType) -> TypeField {
        TypeField {
            label: label.into(),
            label_span: span(),
            ty: HirType {
                kind: HirTypeKind::Constructor(kind),
                span: span(),
            },
            span: span(),
        }
    }

    fn record_function() -> HirType {
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(HirType {
                    kind: HirTypeKind::Record {
                        fields: vec![
                            field("secondValue", BuiltinType::Number),
                            field("first", BuiltinType::Int),
                        ],
                        tail: None,
                    },
                    span: span(),
                }),
                result: Box::new(HirType {
                    kind: HirTypeKind::Constructor(BuiltinType::Unit),
                    span: span(),
                }),
            },
            span: span(),
        }
    }

    #[test]
    fn reuses_a_structurally_equal_record_and_appends_the_function_type() {
        let mut module = module();
        let id = intern_source_type(&mut module, &record_function()).expect("supported signature");
        assert_eq!(id, CoreTypeId(4));
        assert_eq!(
            module.types[4],
            CoreType::Function {
                parameter: CoreTypeId(2),
                result: CoreTypeId(3),
            }
        );
    }

    #[test]
    fn interning_an_equal_type_returns_the_same_id() {
        let mut module = module();
        let first =
            intern_source_type(&mut module, &record_function()).expect("supported signature");
        let length = module.types.len();
        let again =
            intern_source_type(&mut module, &record_function()).expect("supported signature");
        assert_eq!(first, again);
        assert_eq!(module.types.len(), length);
    }

    #[test]
    fn rejects_an_open_record_signature() {
        let mut module = module();
        let open = HirType {
            kind: HirTypeKind::Record {
                fields: vec![field("first", BuiltinType::Int)],
                tail: Some(Box::new(HirType {
                    kind: HirTypeKind::Variable("r".into()),
                    span: span(),
                })),
            },
            span: span(),
        };
        assert!(intern_source_type(&mut module, &open).is_none());
    }
}
