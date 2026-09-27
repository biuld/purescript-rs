//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.
//!
//! The declaration's resolved source type is interned in the Core type table
//! and carried as a `type_id`. CC derives the abstract signature directly from
//! that Core type, so a foreign signature shares the canonical layout of the
//! same type used elsewhere in the module. It also derives a [`PayloadNode`]
//! tree so the canonical adapter can lay out an aggregate payload that is
//! itself a record, list, or variant (DEC-13).

use super::{RefShape, Reference, ReprId, Signature, ValueShape};
use psrs_core::{ConstructorInfo, Module as CoreModule, Type as CoreType, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use std::collections::HashMap;

/// The concrete source shape of one WIT parameter or result position, expanded
/// for the mapped aggregate forms so a nested payload can be encoded or decoded.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PayloadNode {
    /// No concrete shape (an unused default).
    #[default]
    None,
    /// A scalar or string value.
    Value(ValueShape),
    /// A closed record with its representation and per-field payloads. A `flags`
    /// type is also a record of Booleans; the WIT kind selects the bit layout.
    Record {
        representation: ReprId,
        fields: Vec<PayloadField>,
    },
    /// A non-byte list with its array representation and element payload.
    List {
        representation: ReprId,
        element: Box<PayloadNode>,
    },
    /// A source variant with its representation and per-case payloads in tag
    /// order. A nullary case is `None`.
    Variant {
        representation: ReprId,
        cases: Vec<Option<PayloadNode>>,
    },
}

/// One record field of a [`PayloadNode`], keyed by its source label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadField {
    pub name: String,
    pub node: PayloadNode,
}

impl PayloadNode {
    pub(crate) fn shape(&self) -> Option<ValueShape> {
        Some(match self {
            PayloadNode::None => return None,
            PayloadNode::Value(shape) => *shape,
            PayloadNode::Record { representation, .. }
            | PayloadNode::List { representation, .. }
            | PayloadNode::Variant { representation, .. } => reference(*representation),
        })
    }

    /// The payload node of a record field by source label.
    pub(crate) fn field(&self, name: &str) -> Option<&PayloadNode> {
        let PayloadNode::Record { fields, .. } = self else {
            return None;
        };
        fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.node)
    }
}

/// The payload tree for every parameter and the result of one external.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExternalPayloads {
    pub parameters: Vec<PayloadNode>,
    pub result: PayloadNode,
}

pub(crate) fn abstract_signature(
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> Option<(Signature, ExternalPayloads)> {
    let type_reprs = type_representations(module, constructor_types);
    let (parameter_ids, result_id) = crate::abi::link::function_parts(module, type_id?)?;
    let mut parameters = Vec::with_capacity(parameter_ids.len());
    let mut nodes = Vec::with_capacity(parameter_ids.len());
    for parameter in parameter_ids {
        let node = payload_node(module, parameter, record_types, array_types, &type_reprs)?;
        parameters.push(node.shape()?);
        nodes.push(node);
    }
    let result_node = payload_node(module, result_id, record_types, array_types, &type_reprs)?;
    Some((
        Signature {
            parameters,
            result: result_node.shape()?,
        },
        ExternalPayloads {
            parameters: nodes,
            result: result_node,
        },
    ))
}

/// Maps each payload-bearing data type declaration to the variant representation
/// the `TypeLayout` assigned to its constructors. Nullary enums, opaque handles,
/// and newtypes are absent.
fn type_representations(
    module: &CoreModule,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> HashMap<HirTypeId, ReprId> {
    module
        .constructors
        .iter()
        .filter_map(|constructor| {
            constructor_types
                .get(&constructor.symbol)
                .map(|repr| (constructor.type_id, *repr))
        })
        .collect()
}

fn payload_node(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<PayloadNode> {
    Some(match module.types.get(id.0 as usize)? {
        CoreType::I32 | CoreType::Char | CoreType::Unit => PayloadNode::Value(ValueShape::Integer),
        CoreType::Boolean => PayloadNode::Value(ValueShape::Boolean),
        CoreType::F64 => PayloadNode::Value(ValueShape::Number),
        CoreType::String => PayloadNode::Value(ValueShape::String),
        CoreType::Record(fields) => {
            let representation = *record_types.get(&id)?;
            let fields = fields
                .iter()
                .map(|(name, field)| {
                    payload_node(module, *field, record_types, array_types, type_reprs).map(
                        |node| PayloadField {
                            name: name.clone(),
                            node,
                        },
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            PayloadNode::Record {
                representation,
                fields,
            }
        }
        CoreType::Application(function, element)
            if matches!(
                module.types.get(function.0 as usize),
                Some(CoreType::Constructor(TypeConstructor::Array))
            ) =>
        {
            PayloadNode::List {
                representation: *array_types.get(&id)?,
                element: Box::new(payload_node(
                    module,
                    *element,
                    record_types,
                    array_types,
                    type_reprs,
                )?),
            }
        }
        CoreType::Constructor(TypeConstructor::User(_)) | CoreType::Application(_, _) => {
            user_payload_node(module, id, record_types, array_types, type_reprs)?
        }
        _ => return None,
    })
}

fn user_payload_node(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<PayloadNode> {
    let head = head_user_type(module, id)?;
    let constructors = constructors_of(module, head);
    // A nullary enum or an opaque handle is an `i32`.
    if constructors.is_empty() || constructors.iter().all(|case| case.field_count == 0) {
        return Some(PayloadNode::Value(ValueShape::Integer));
    }
    let representation = *type_reprs.get(&head)?;
    let arguments = application_arguments(module, id);
    let nested = |ty: TypeId| payload_node(module, ty, record_types, array_types, type_reprs);
    let cases = match user_type_name(module, head) {
        Some("Data.Maybe.Maybe") if arguments.len() == 1 => {
            vec![None, Some(nested(arguments[0])?)]
        }
        Some("Data.Either.Either") if arguments.len() == 2 => {
            vec![Some(nested(arguments[0])?), Some(nested(arguments[1])?)]
        }
        _ => constructors
            .iter()
            .map(|case| match case.field_types.first() {
                Some(field) => nested(*field).map(Some),
                None => Some(None),
            })
            .collect::<Option<Vec<_>>>()?,
    };
    Some(PayloadNode::Variant {
        representation,
        cases,
    })
}

fn head_user_type(module: &CoreModule, mut id: TypeId) -> Option<HirTypeId> {
    loop {
        match module.types.get(id.0 as usize)? {
            CoreType::Constructor(TypeConstructor::User(type_id)) => return Some(*type_id),
            CoreType::Application(function, _) => id = *function,
            _ => return None,
        }
    }
}

fn application_arguments(module: &CoreModule, mut id: TypeId) -> Vec<TypeId> {
    let mut arguments = Vec::new();
    while let Some(CoreType::Application(function, argument)) = module.types.get(id.0 as usize) {
        arguments.push(*argument);
        id = *function;
    }
    arguments.reverse();
    arguments
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

fn user_type_name(module: &CoreModule, type_id: HirTypeId) -> Option<&str> {
    module
        .type_names
        .iter()
        .find(|(candidate, _)| *candidate == type_id)
        .map(|(_, name)| name.as_str())
}

fn reference(representation: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    })
}
