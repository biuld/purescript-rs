//! Maps resolved WIT types to the source ABI subset and canonical value types.

use super::flatten::primitive_aggregate_allowed;
use super::{SourceSignature, SourceType, WasiField, WasiParamKind, WasiResultKind};
use crate::types::ValueType;
use psrs_core::Module as CoreModule;
use psrs_hir::{BuiltinType, Type as HirType, TypeId as HirTypeId, TypeKind as HirTypeKind};
use wit_parser::abi::WasmType;
use wit_parser::{Handle, Resolve, Type as WitType, TypeDefKind};

/// Converts a resolved HIR foreign-import type into the source-level subset
/// that may cross into CC. Unsupported polymorphic, aggregate, or higher-kinded
/// declarations remain `None` and are rejected by the ABI validation pass.
pub(crate) fn source_signature(
    module: &CoreModule,
    signature: &HirType,
) -> Option<SourceSignature> {
    let mut parameters = Vec::new();
    let mut result = signature;
    while let HirTypeKind::Function {
        parameter,
        result: next,
    } = &result.kind
    {
        parameters.push(source_type(module, parameter)?);
        result = next.as_ref();
    }
    Some(SourceSignature {
        parameters,
        result: source_type(module, result)?,
        span: signature.span,
    })
}

fn source_type(module: &CoreModule, ty: &HirType) -> Option<SourceType> {
    match &ty.kind {
        HirTypeKind::Constructor(BuiltinType::Int) => Some(SourceType::Int),
        HirTypeKind::Constructor(BuiltinType::Boolean) => Some(SourceType::Boolean),
        HirTypeKind::Constructor(BuiltinType::Number) => Some(SourceType::Number),
        HirTypeKind::Constructor(BuiltinType::Char) => Some(SourceType::Char),
        HirTypeKind::Constructor(BuiltinType::String) => Some(SourceType::String),
        HirTypeKind::Constructor(BuiltinType::Unit) => Some(SourceType::Unit),
        HirTypeKind::Opaque(type_id) => Some(SourceType::Resource { type_id: *type_id }),
        HirTypeKind::Named(type_id) => source_enum_type(module, *type_id),
        HirTypeKind::Application(function, argument) => {
            if is_source_array(function) {
                let element = source_type(module, argument)?;
                return array_source(element);
            }
            let type_id = user_type_id(function)?;
            source_enum_type(module, type_id)
        }
        HirTypeKind::Record { fields, tail } if tail.is_none() => {
            let mut fields = fields
                .iter()
                .map(|field| {
                    Some((
                        field.label.clone(),
                        Box::new(source_type(module, &field.ty)?),
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            Some(SourceType::Record { fields })
        }
        _ => None,
    }
}

fn is_source_array(ty: &HirType) -> bool {
    matches!(ty.kind, HirTypeKind::Constructor(BuiltinType::Array))
}

/// Arrays cross the ABI only for elements that already have a scalar or string
/// lowering. Nested arrays, records, and handles stay unsupported.
fn array_source(element: SourceType) -> Option<SourceType> {
    match element {
        SourceType::Int
        | SourceType::Boolean
        | SourceType::Number
        | SourceType::Char
        | SourceType::String => Some(SourceType::Array {
            element: Box::new(element),
        }),
        _ => None,
    }
}

fn user_type_id(ty: &HirType) -> Option<HirTypeId> {
    match &ty.kind {
        HirTypeKind::Named(type_id) => Some(*type_id),
        HirTypeKind::Application(function, _) => user_type_id(function),
        _ => None,
    }
}

fn source_enum_type(module: &CoreModule, type_id: HirTypeId) -> Option<SourceType> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    if constructors.is_empty()
        || constructors
            .iter()
            .any(|constructor| constructor.field_count != 0)
        || constructors
            .iter()
            .enumerate()
            .any(|(index, constructor)| constructor.tag != index as u32)
    {
        return None;
    }
    Some(SourceType::Enum {
        cases: constructors
            .into_iter()
            .map(|constructor| constructor.name.clone())
            .collect(),
    })
}

pub(super) fn unsupported_shape(
    resolve: &Resolve,
    function: &wit_parser::Function,
    result_kind: &WasiResultKind,
) -> Option<String> {
    if function
        .params
        .iter()
        .any(|parameter| contains_rejected_list(resolve, &parameter.ty, false))
    {
        return Some("non-byte WIT lists are not supported by the String ABI".into());
    }
    // `option`, tuple, and payload-bearing variant stay `Unsupported` as a WIT
    // parameter kind. A primitive import of those shapes is checked later,
    // against the canonical flat slots, so this pass must not reject them
    // before the source signature is known. Maps and other non-flat shapes
    // still have no source ABI.
    if function.params.iter().any(|parameter| {
        param_kind(resolve, &parameter.ty) == WasiParamKind::Unsupported
            && !primitive_aggregate_allowed(resolve, &parameter.ty)
    }) {
        return Some("WIT parameter shape has no source ABI mapping yet".into());
    }
    if let Some(result) = &function.result
        && contains_rejected_list(resolve, result, false)
        && !list_is_bytes(resolve, result)
    {
        return Some("non-byte WIT list results are not supported by the String ABI".into());
    }
    if matches!(result_kind, WasiResultKind::Result)
        && let Some(result) = &function.result
        && !result_has_unit_ok(resolve, result)
    {
        return Some("WIT result with a payload on success has no source ABI mapping yet".into());
    }
    if matches!(result_kind, WasiResultKind::Discarded) {
        return Some("WIT result shape has no source ABI mapping yet".into());
    }
    None
}

fn result_has_unit_ok(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Result(result) => result.ok.is_none(),
            TypeDefKind::Type(inner) => result_has_unit_ok(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

fn list_is_bytes(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::String | WitType::U8 => true,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(inner) | TypeDefKind::FixedLengthList(inner, ..) => {
                list_element_is_bytes(resolve, inner)
            }
            TypeDefKind::Type(inner) => list_is_bytes(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

/// Classifies the element of a variable-length list. Byte elements stay the
/// string list. A string or `list<u8>` element is one string inside the outer
/// list. Another list, a record, a handle, or `option` is unsupported.
fn classify_variable_list(resolve: &Resolve, inner: &WitType) -> WasiParamKind {
    if list_element_is_bytes(resolve, inner) {
        return WasiParamKind::List;
    }
    match classify_list_element(resolve, inner) {
        kind @ (WasiParamKind::Integer32
        | WasiParamKind::IntegerNarrow { .. }
        | WasiParamKind::Boolean
        | WasiParamKind::Char
        | WasiParamKind::Scalar64 { .. }
        | WasiParamKind::Float32
        | WasiParamKind::Float64
        | WasiParamKind::List) => WasiParamKind::ValueList {
            element: Box::new(kind),
        },
        _ => WasiParamKind::Unsupported,
    }
}

fn classify_list_element(resolve: &Resolve, ty: &WitType) -> WasiParamKind {
    if string_like_element(resolve, ty) {
        return WasiParamKind::List;
    }
    if is_list_type(resolve, ty) {
        return WasiParamKind::Unsupported;
    }
    param_kind(resolve, ty)
}

fn string_like_element(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::String => true,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Type(inner) => string_like_element(resolve, inner),
            TypeDefKind::List(inner) | TypeDefKind::FixedLengthList(inner, _) => {
                list_element_is_bytes(resolve, inner)
            }
            _ => false,
        },
        _ => false,
    }
}

fn is_list_type(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(_) | TypeDefKind::FixedLengthList(..) => true,
            TypeDefKind::Type(inner) => is_list_type(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

fn list_element_is_bytes(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::U8 => true,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Type(inner) => list_element_is_bytes(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

/// A top-level `list<T>` of a supported element is lowered. The same list nested
/// in a record, tuple, option, result, or variant is still rejected, as is any
/// list whose element has no scalar or string lowering.
fn contains_rejected_list(resolve: &Resolve, ty: &WitType, nested: bool) -> bool {
    match ty {
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(inner) => {
                !(list_element_is_bytes(resolve, inner)
                    || (!nested && supported_list_element(resolve, inner)))
            }
            TypeDefKind::FixedLengthList(inner, _) => !list_element_is_bytes(resolve, inner),
            TypeDefKind::Record(record) => record
                .fields
                .iter()
                .any(|field| contains_rejected_list(resolve, &field.ty, true)),
            TypeDefKind::Tuple(tuple) => tuple
                .types
                .iter()
                .any(|ty| contains_rejected_list(resolve, ty, true)),
            TypeDefKind::Option(inner) => contains_rejected_list(resolve, inner, true),
            TypeDefKind::Result(result) => {
                result
                    .ok
                    .as_ref()
                    .is_some_and(|ty| contains_rejected_list(resolve, ty, true))
                    || result
                        .err
                        .as_ref()
                        .is_some_and(|ty| contains_rejected_list(resolve, ty, true))
            }
            TypeDefKind::Variant(variant) => variant.cases.iter().any(|case| {
                case.ty
                    .as_ref()
                    .is_some_and(|ty| contains_rejected_list(resolve, ty, true))
            }),
            TypeDefKind::Type(inner) => contains_rejected_list(resolve, inner, nested),
            _ => false,
        },
        _ => false,
    }
}

fn supported_list_element(resolve: &Resolve, ty: &WitType) -> bool {
    matches!(
        classify_list_element(resolve, ty),
        WasiParamKind::Integer32
            | WasiParamKind::IntegerNarrow { .. }
            | WasiParamKind::Boolean
            | WasiParamKind::Char
            | WasiParamKind::Scalar64 { .. }
            | WasiParamKind::Float32
            | WasiParamKind::Float64
            | WasiParamKind::List
    )
}

/// Classifies a WIT-level parameter so lowering knows how many canonical
/// parameters it produces. Aliases are followed.
pub(super) fn param_kind(resolve: &Resolve, ty: &WitType) -> WasiParamKind {
    match ty {
        WitType::Bool => WasiParamKind::Boolean,
        WitType::S32 | WitType::U32 => WasiParamKind::Integer32,
        WitType::U8 => WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: false,
        },
        WitType::S8 => WasiParamKind::IntegerNarrow {
            bits: 8,
            signed: true,
        },
        WitType::U16 => WasiParamKind::IntegerNarrow {
            bits: 16,
            signed: false,
        },
        WitType::S16 => WasiParamKind::IntegerNarrow {
            bits: 16,
            signed: true,
        },
        WitType::F64 => WasiParamKind::Float64,
        WitType::String => WasiParamKind::List,
        WitType::Char => WasiParamKind::Char,
        WitType::F32 => WasiParamKind::Float32,
        WitType::U64 => WasiParamKind::Scalar64 { signed: false },
        WitType::S64 => WasiParamKind::Scalar64 { signed: true },
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(inner) => classify_variable_list(resolve, inner),
            TypeDefKind::FixedLengthList(inner, _) => {
                if list_element_is_bytes(resolve, inner) {
                    WasiParamKind::List
                } else {
                    WasiParamKind::Unsupported
                }
            }
            TypeDefKind::Handle(handle) => classify_handle(resolve, handle),
            TypeDefKind::Enum(enum_) => WasiParamKind::Enum {
                cases: enum_
                    .cases
                    .iter()
                    .map(|case| source_constructor_name(&case.name))
                    .collect(),
            },
            TypeDefKind::Flags(flags) => WasiParamKind::Flags {
                names: flags.flags.iter().map(|flag| flag.name.clone()).collect(),
            },
            TypeDefKind::Record(record) => {
                let fields = record
                    .fields
                    .iter()
                    .map(|field| WasiField {
                        name: field.name.clone(),
                        kind: param_kind(resolve, &field.ty),
                    })
                    .collect::<Vec<_>>();
                if fields.iter().all(|field| direct_parameter(&field.kind)) {
                    WasiParamKind::Record { fields }
                } else {
                    WasiParamKind::Unsupported
                }
            }
            TypeDefKind::Type(inner) => param_kind(resolve, inner),
            _ => WasiParamKind::Unsupported,
        },
        _ => WasiParamKind::Unsupported,
    }
}

fn direct_parameter(kind: &WasiParamKind) -> bool {
    match kind {
        WasiParamKind::Integer32
        | WasiParamKind::IntegerNarrow { .. }
        | WasiParamKind::Boolean
        | WasiParamKind::Char
        | WasiParamKind::Scalar64 { .. }
        | WasiParamKind::Float32
        | WasiParamKind::Float64
        | WasiParamKind::Enum { .. }
        | WasiParamKind::Flags { .. }
        | WasiParamKind::Handle(_) => true,
        WasiParamKind::Record { fields } => {
            fields.iter().all(|field| direct_parameter(&field.kind))
        }
        WasiParamKind::List | WasiParamKind::ValueList { .. } => true,
        WasiParamKind::Unsupported => false,
    }
}

/// Classifies a WIT result as a direct value, indirect value, or unsupported
/// shape. Aliases are followed.
pub(super) fn result_kind(resolve: &Resolve, ty: &WitType) -> WasiResultKind {
    match ty {
        WitType::String => WasiResultKind::List,
        WitType::Char => WasiResultKind::Char,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(inner) => match classify_variable_list(resolve, inner) {
                WasiParamKind::List => WasiResultKind::List,
                WasiParamKind::ValueList { element } => WasiResultKind::ValueList { element },
                _ => WasiResultKind::Discarded,
            },
            TypeDefKind::FixedLengthList(inner, _) => {
                if list_element_is_bytes(resolve, inner) {
                    WasiResultKind::List
                } else {
                    WasiResultKind::Discarded
                }
            }
            TypeDefKind::Handle(handle) => classify_result_handle(resolve, handle),
            TypeDefKind::Result(_) => WasiResultKind::Result,
            TypeDefKind::Enum(enum_) => WasiResultKind::Enum {
                cases: enum_
                    .cases
                    .iter()
                    .map(|case| source_constructor_name(&case.name))
                    .collect(),
            },
            TypeDefKind::Type(inner) => result_kind(resolve, inner),
            _ => WasiResultKind::Discarded,
        },
        WitType::Bool => WasiResultKind::Boolean,
        WitType::S32 | WitType::U32 | WitType::S64 | WitType::U64 | WitType::F32 | WitType::F64 => {
            WasiResultKind::Scalar
        }
        WitType::U8 => WasiResultKind::IntegerNarrow {
            bits: 8,
            signed: false,
        },
        WitType::S8 => WasiResultKind::IntegerNarrow {
            bits: 8,
            signed: true,
        },
        WitType::U16 => WasiResultKind::IntegerNarrow {
            bits: 16,
            signed: false,
        },
        WitType::S16 => WasiResultKind::IntegerNarrow {
            bits: 16,
            signed: true,
        },
        _ => WasiResultKind::Discarded,
    }
}

fn classify_handle(resolve: &Resolve, handle: &Handle) -> WasiParamKind {
    super::handles::classify(resolve, handle)
        .map(WasiParamKind::Handle)
        .unwrap_or(WasiParamKind::Unsupported)
}

fn classify_result_handle(resolve: &Resolve, handle: &Handle) -> WasiResultKind {
    super::handles::classify(resolve, handle)
        .map(WasiResultKind::Handle)
        .unwrap_or(WasiResultKind::Discarded)
}

fn source_constructor_name(wit_case: &str) -> String {
    wit_case
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            let mut name = String::new();
            if let Some(first) = chars.next() {
                name.extend(first.to_uppercase());
                name.extend(chars);
            }
            name
        })
        .collect()
}

pub(super) fn value_type(ty: WasmType) -> Result<ValueType, String> {
    Ok(match ty {
        WasmType::I32 | WasmType::Pointer | WasmType::Length => ValueType::I32,
        WasmType::I64 | WasmType::PointerOrI64 => ValueType::I64,
        WasmType::F32 => ValueType::F32,
        WasmType::F64 => ValueType::F64,
    })
}
