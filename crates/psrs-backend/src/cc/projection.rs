//! The instance-aware ABI guest projection.
//!
//! One recursive walk over a resolved Core type and the representation table
//! produces a [`GuestLayout`] whose every field pairs the concrete guest value
//! (`value`) with the physical storage slot it maps to (`stored`). Physical
//! layout, tag order, field order, labels, and storage shapes come from the
//! [`RepresentationTable`]; the concrete value comes from the Core type with the
//! enclosing application's type arguments substituted into each constructor's
//! field templates (the correspondence carried by `ConstructorInfo::parameters`).
//!
//! A missing argument, a non-closed type, or a storage/field-count mismatch is a
//! binding error. `Maybe` and `Either` names are not consulted here: they only
//! matter to the WIT conformance check.

use super::{
    Field, GuestCase, GuestLayout, RefShape, Reference, ReprId, Representation,
    RepresentationTable, ValueShape,
};
use psrs_core::{ConstructorInfo, Module as CoreModule, Type, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId, TypeVariableId};
use std::collections::HashMap;

/// The instantiated guest projection of one external binding: one layout per
/// source parameter and the result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalProjection {
    pub parameters: Vec<GuestLayout>,
    pub result: Option<GuestLayout>,
}

pub(crate) struct Projector<'a> {
    module: &'a CoreModule,
    record_types: &'a HashMap<TypeId, ReprId>,
    array_types: &'a HashMap<TypeId, ReprId>,
    constructor_types: &'a HashMap<SymbolId, ReprId>,
    table: &'a RepresentationTable,
}

type Env = HashMap<TypeVariableId, TypeId>;

/// Projects the declared parameters and result of an external's resolved Core
/// type. `Ok(None)` when the declaration is not a function type, matching the
/// abstract signature that also cannot resolve it.
pub(crate) fn project_external(
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
    table: &RepresentationTable,
) -> Result<Option<ExternalProjection>, String> {
    let Some(type_id) = type_id else {
        return Ok(None);
    };
    let Some((parameters, result)) = crate::abi::link::function_parts(module, type_id) else {
        return Ok(None);
    };
    let projector = Projector {
        module,
        record_types,
        array_types,
        constructor_types,
        table,
    };
    let parameters = parameters
        .into_iter()
        .map(|parameter| projector.project(parameter, &Env::new()))
        .collect::<Result<Vec<_>, _>>()?;
    let result = projector.project(result, &Env::new())?;
    Ok(Some(ExternalProjection {
        parameters,
        result: Some(result),
    }))
}

impl Projector<'_> {
    fn project(&self, id: TypeId, env: &Env) -> Result<GuestLayout, String> {
        // A callable closure — an arrow or a callable constructor application —
        // has no canonical guest layout.
        if super::layout::is_callable_type(self.module, id) {
            return Err("function types have no canonical guest layout".into());
        }
        match self.module.types.get(id.0 as usize) {
            Some(Type::I32 | Type::Char | Type::Unit) => Ok(scalar(ValueShape::Integer)),
            Some(Type::Boolean) => Ok(scalar(ValueShape::Boolean)),
            Some(Type::F64) => Ok(scalar(ValueShape::Number)),
            Some(Type::String) => Ok(scalar(ValueShape::String)),
            Some(Type::Variable(variable)) => {
                let Some(substituted) = env.get(variable) else {
                    return Err("guest projection of a non-closed type".into());
                };
                self.project(*substituted, env)
            }
            Some(Type::Record(_)) => self.project_record(id, env),
            Some(Type::Application(_, _)) => {
                if let Some(element) = super::layout::array_element_type(self.module, id) {
                    self.project_array(id, element, env)
                } else {
                    self.project_user(id, env)
                }
            }
            Some(Type::Constructor(TypeConstructor::User(_))) => self.project_user(id, env),
            Some(Type::OpenRecord { .. }) => {
                Err("open record rows have no runtime guest layout".into())
            }
            Some(Type::Constructor(TypeConstructor::Array)) => {
                Err("an unapplied Array has no guest layout".into())
            }
            Some(Type::Constructor(TypeConstructor::Function)) => {
                Err("function types have no canonical guest layout".into())
            }
            None => Err("type is outside the Core type table".into()),
        }
    }

    fn project_record(&self, id: TypeId, env: &Env) -> Result<GuestLayout, String> {
        let repr = *self
            .record_types
            .get(&id)
            .ok_or_else(|| "record has no representation".to_string())?;
        let Representation::Product { fields: stored } = self
            .table
            .representation(repr)
            .ok_or_else(|| "record representation is missing".to_string())?
        else {
            return Err("record representation is not a product".into());
        };
        let labels = self
            .table
            .product_labels(repr)
            .ok_or_else(|| "record representation has no labels".to_string())?;
        let Some(Type::Record(core_fields)) = self.module.types.get(id.0 as usize) else {
            return Err("record type changed during projection".into());
        };
        if labels.len() != stored.len() {
            return Err("record label and storage field counts disagree".into());
        }
        let mut fields = Vec::with_capacity(labels.len());
        for (label, stored_shape) in labels.iter().zip(stored) {
            let Some((_, core_field)) =
                core_fields.iter().find(|(candidate, _)| candidate == label)
            else {
                return Err(format!("record has no field `{label}`"));
            };
            let value = self.project(*core_field, env)?;
            fields.push(field(value, *stored_shape)?);
        }
        Ok(GuestLayout::Product {
            repr,
            labels: labels.to_vec(),
            fields,
        })
    }

    fn project_array(&self, id: TypeId, element: TypeId, env: &Env) -> Result<GuestLayout, String> {
        let repr = *self
            .array_types
            .get(&id)
            .ok_or_else(|| "array has no representation".to_string())?;
        let Representation::Array { element: stored } = self
            .table
            .representation(repr)
            .ok_or_else(|| "array representation is missing".to_string())?
        else {
            return Err("array representation is not an array".into());
        };
        Ok(GuestLayout::Array {
            repr,
            element: Box::new(field(self.project(element, env)?, *stored)?),
        })
    }

    fn project_user(&self, id: TypeId, env: &Env) -> Result<GuestLayout, String> {
        let (hir, arguments) = applied_parts(self.module, id)
            .ok_or_else(|| "user type is not an applied constructor".to_string())?;
        if self.module.opaque_ids.contains(&hir) {
            return Ok(scalar(ValueShape::Integer));
        }
        let constructors = constructors_of(self.module, hir);
        if constructors.is_empty() || constructors.iter().all(|case| case.field_count == 0) {
            return Ok(scalar(ValueShape::Integer));
        }
        if self.module.newtype_ids.contains(&hir) {
            let constructor = constructors[0];
            let env = self.constructor_env(constructor, &arguments, env)?;
            let field = constructor
                .field_types
                .first()
                .ok_or_else(|| "newtype has no field".to_string())?;
            return self.project(*field, &env);
        }
        let repr = *self
            .constructor_types
            .get(&constructors[0].symbol)
            .ok_or_else(|| "user type has no representation".to_string())?;
        let Representation::Variant { cases } = self
            .table
            .representation(repr)
            .ok_or_else(|| "user type representation is missing".to_string())?
        else {
            return Err("user type representation is not a variant".into());
        };
        if cases.len() != constructors.len() {
            return Err("variant case count disagrees with the declaration".into());
        }
        let mut guest_cases = Vec::with_capacity(cases.len());
        for (constructor, case) in constructors.iter().zip(cases) {
            if constructor.field_types.len() != case.fields.len() {
                return Err("variant case field count disagrees with the declaration".into());
            }
            let env = self.constructor_env(constructor, &arguments, env)?;
            let mut fields = Vec::with_capacity(case.fields.len());
            for (template, stored) in constructor.field_types.iter().zip(&case.fields) {
                let value = self.project(*template, &env)?;
                fields.push(field(value, *stored)?);
            }
            guest_cases.push(GuestCase {
                tag: case.tag,
                fields,
            });
        }
        Ok(GuestLayout::Variant {
            repr,
            cases: guest_cases,
        })
    }

    /// The substitution that maps a constructor's declared type parameters to
    /// the enclosing application's arguments. The outer environment is retained
    /// so an argument that is itself an enclosing type variable still resolves.
    fn constructor_env(
        &self,
        constructor: &ConstructorInfo,
        arguments: &[TypeId],
        outer: &Env,
    ) -> Result<Env, String> {
        if constructor.parameters.len() != arguments.len() {
            return Err(format!(
                "type application has {} argument(s) but {} parameter(s) were declared",
                arguments.len(),
                constructor.parameters.len()
            ));
        }
        let mut env = outer.clone();
        for (parameter, argument) in constructor.parameters.iter().zip(arguments) {
            env.insert(*parameter, *argument);
        }
        Ok(env)
    }
}

fn scalar(shape: ValueShape) -> GuestLayout {
    GuestLayout::Scalar { shape }
}

/// Pairs a projected concrete value with its storage slot, checking that the
/// two are related by a defined CC conversion. They must be identical, or the
/// storage slot must be an erased or aggregate supertype that the box/cast
/// protocol bridges.
fn field(value: GuestLayout, stored: ValueShape) -> Result<Field, String> {
    let converted = value.shape() == stored
        || match stored {
            ValueShape::Reference(Reference {
                heap: RefShape::Erased,
                ..
            }) => true,
            ValueShape::Reference(Reference {
                heap: RefShape::Aggregate,
                ..
            }) => matches!(
                value,
                GuestLayout::Product { .. }
                    | GuestLayout::Variant { .. }
                    | GuestLayout::Array { .. }
                    | GuestLayout::Boxed { .. }
            ),
            _ => false,
        };
    if !converted {
        return Err(format!(
            "guest field storage {stored:?} has no CC conversion from {value:?}"
        ));
    }
    Ok(Field { value, stored })
}

fn applied_parts(module: &CoreModule, mut id: TypeId) -> Option<(HirTypeId, Vec<TypeId>)> {
    let mut arguments = Vec::new();
    while let Some(Type::Application(function, argument)) = module.types.get(id.0 as usize) {
        arguments.push(*argument);
        id = *function;
    }
    arguments.reverse();
    match module.types.get(id.0 as usize) {
        Some(Type::Constructor(TypeConstructor::User(hir))) => Some((*hir, arguments)),
        _ => None,
    }
}

fn constructors_of(module: &CoreModule, type_id: HirTypeId) -> Vec<&ConstructorInfo> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    constructors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::{RefShape, Reference, ValueShape};
    use psrs_core::{ConstructorInfo, Module, Type, TypeConstructor};
    use psrs_hir::{ModuleId, TypeVariableId};

    /// `Maybe { name :: String }` projects to a `Maybe` case whose field records
    /// the concrete `{ name :: String }` product as `value` and the erased
    /// storage slot as `stored`.
    #[test]
    fn projects_a_nested_parameterized_payload() {
        let module_id = ModuleId(0);
        let maybe = HirTypeId::new(module_id, 0);
        let record = TypeId(0);
        let string = TypeId(1);
        let maybe_ctor = TypeId(2);
        let maybe_record = TypeId(3);
        let unit = TypeId(4);
        let just = SymbolId::new(module_id, 0);
        let nothing = SymbolId::new(module_id, 1);
        let type_variable = TypeVariableId(7);
        let mut types = vec![
            Type::Record(vec![("name".into(), string)]),
            Type::String,
            Type::Constructor(TypeConstructor::User(maybe)),
            Type::Application(maybe_ctor, record),
            Type::Unit,
        ];
        // The `Just` field template names the type variable.
        let variable = TypeId(types.len() as u32);
        types.push(Type::Variable(type_variable));
        let head = TypeId(types.len() as u32);
        types.push(Type::Constructor(TypeConstructor::Function));
        let inner = TypeId(types.len() as u32);
        types.push(Type::Application(head, unit));
        let function = TypeId(types.len() as u32);
        types.push(Type::Application(inner, maybe_record));
        let module = Module {
            id: module_id,
            name: "ProjectionTest".into(),
            externals: Vec::new(),
            types,
            newtype_ids: Vec::new(),
            opaque_ids: Vec::new(),
            callable_types: Vec::new(),
            type_names: vec![(maybe, "Data.Maybe.Maybe".into())],
            constructors: vec![
                ConstructorInfo {
                    symbol: just,
                    name: "Just".into(),
                    type_id: maybe,
                    tag: 1,
                    field_count: 1,
                    field_types: vec![variable],
                    parameters: vec![type_variable],
                },
                ConstructorInfo {
                    symbol: nothing,
                    name: "Nothing".into(),
                    type_id: maybe,
                    tag: 0,
                    field_count: 0,
                    field_types: Vec::new(),
                    parameters: vec![type_variable],
                },
            ],
            declarations: Vec::new(),
            entry: None,
            span: psrs_span::TextRange::new(0, 0),
        };
        assert_eq!(
            module.types.get(variable.0 as usize),
            Some(&Type::Variable(type_variable))
        );
        let erased = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        });
        let representations = RepresentationTable {
            representations: vec![
                Representation::Product {
                    fields: vec![ValueShape::String],
                },
                Representation::Variant {
                    cases: vec![
                        crate::cc::VariantCase {
                            tag: 0,
                            fields: Vec::new(),
                        },
                        crate::cc::VariantCase {
                            tag: 1,
                            fields: vec![erased],
                        },
                    ],
                },
            ],
            signatures: Vec::new(),
            product_labels: [(ReprId(0), vec!["name".to_string()])]
                .into_iter()
                .collect(),
        };
        let record_types = [(record, ReprId(0))].into_iter().collect();
        let array_types = HashMap::new();
        let constructor_types = [(just, ReprId(1)), (nothing, ReprId(1))]
            .into_iter()
            .collect();
        let projection = project_external(
            Some(function),
            &module,
            &record_types,
            &array_types,
            &constructor_types,
            &representations,
        )
        .expect("the projection should succeed")
        .expect("a function type projects");
        assert_eq!(projection.parameters.len(), 1);
        let Some(GuestLayout::Variant { cases, .. }) = projection.result else {
            panic!("the result should be a variant");
        };
        assert_eq!(cases.len(), 2);
        assert!(cases[0].fields.is_empty());
        let field = &cases[1].fields[0];
        assert_eq!(field.stored, erased);
        assert!(matches!(
            &field.value,
            GuestLayout::Product { labels, .. } if labels == &["name".to_string()]
        ));
    }
}
