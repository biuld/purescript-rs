use crate::abi::{SourceSignature, SourceType};
use crate::{BackendError, BackendInput, ExternalBindings, annotate_errors};
use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod case;
mod layout;
mod lower;
mod representation;
mod scalar;
mod verify;

use layout::{aggregate_type_ids, declaration_shape, enum_type_ids, type_layout};
use lower::{GeneratedSymbolAllocator, LoweringContext, lower_function};

pub use crate::types::ValueId;
pub use representation::{
    RefShape, Reference, ReprId, Representation, RepresentationTable, Signature, SignatureId,
    ValueDecl, ValueShape, VariantCase,
};
pub use scalar::{BinaryOp, UnaryOp};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub externals: Vec<External>,
    /// Target-neutral representation requirements. P9 owns their concrete
    /// layout and the Wasm type table.
    pub representations: RepresentationTable,
    pub functions: Vec<Function>,
    /// The program entry declaration, if selected by the driver. A stable symbol
    /// rather than a source name, per `docs/design/D-02-wasm-lowering.md`.
    pub entry: Option<SymbolId>,
    pub span: TextRange,
}

/// A target-neutral external declaration. Platform binding metadata is kept in
/// [`ExternalBindings`] and is not part of CC identity or equality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub symbol: SymbolId,
    pub signature: Option<Signature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub parameters: Vec<ValueId>,
    pub values: Vec<ValueDecl>,
    pub assignments: Vec<Assignment>,
    pub result: ValueId,
    pub result_type: ValueShape,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub destination: ValueId,
    pub kind: AssignmentKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentKind {
    Constant(i32),
    NumberConstant(String),
    StringConstant(String),
    Primitive {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    Unary {
        op: UnaryOp,
        value: ValueId,
    },
    DirectCall {
        function: SymbolId,
        arguments: Vec<ValueId>,
    },
    FunctionRef {
        function: SymbolId,
        signature: SignatureId,
        captures: Vec<ValueId>,
    },
    IndirectCall {
        function: ValueId,
        signature: SignatureId,
        arguments: Vec<ValueId>,
    },
    ClosureGetCapture {
        closure: ValueId,
        index: u32,
    },
    RepresentationTest {
        destination: ValueId,
        value: ValueId,
        reference: Reference,
    },
    RepresentationCast {
        destination: ValueId,
        value: ValueId,
        reference: Reference,
    },
    ProductNew {
        destination: ValueId,
        representation: ReprId,
        arguments: Vec<ValueId>,
    },
    ProductGet {
        destination: ValueId,
        representation: ReprId,
        field: u32,
        value: ValueId,
    },
    VariantNew {
        destination: ValueId,
        representation: ReprId,
        case: u32,
        fields: Vec<ValueId>,
    },
    VariantTag {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    VariantGet {
        destination: ValueId,
        representation: ReprId,
        case: u32,
        field: u32,
        value: ValueId,
    },
    ArrayNew {
        destination: ValueId,
        representation: ReprId,
        elements: Vec<ValueId>,
    },
    ArrayLen {
        destination: ValueId,
        value: ValueId,
    },
    ArrayGet {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        index: ValueId,
    },
    ArrayClone {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    ArraySet {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        index: ValueId,
        new_value: ValueId,
    },
    If {
        condition: ValueId,
        then_assignments: Vec<Assignment>,
        then_value: ValueId,
        else_assignments: Vec<Assignment>,
        else_value: ValueId,
    },
}

/// Lowers Core with the default backend-side external binding extraction.
/// Prefer [`lower_module_with_bindings`] when the caller already owns the
/// backend input boundary.
pub fn lower_module(module: CoreModule) -> Result<BackendInput, Vec<BackendError>> {
    let bindings = ExternalBindings::from_core(&module);
    lower_module_with_bindings(module, bindings)
}

/// Lowers Core into target-neutral CC using bindings supplied beside CC.
pub fn lower_module_with_bindings(
    module: CoreModule,
    bindings: ExternalBindings,
) -> Result<BackendInput, Vec<BackendError>> {
    bindings.validate_core(&module)?;
    if let Err(errors) = module.verify() {
        return Err(annotate_errors(
            errors
                .into_iter()
                .map(|error| BackendError::new("P8 Core verification", error.span, error.message))
                .collect(),
            module.entry.map(|entry| entry.module),
        ));
    }
    let newtype_ids = module.newtype_ids.iter().copied().collect();
    let enum_types = enum_type_ids(&module, &newtype_ids);
    let aggregate_types = aggregate_type_ids(&module, &newtype_ids);
    let layout = type_layout(&module, &enum_types, &aggregate_types, &newtype_ids)?;
    let mut constructor_tags = HashMap::new();
    let mut constructors_by_type: HashMap<HirTypeId, Vec<(SymbolId, u32)>> = HashMap::new();
    for constructor in &module.constructors {
        constructor_tags.insert(constructor.symbol, constructor.tag);
        constructors_by_type
            .entry(constructor.type_id)
            .or_default()
            .push((constructor.symbol, constructor.tag));
    }
    let mut signatures = HashMap::new();
    for declaration in &module.declarations {
        let signature = declaration_shape(
            declaration,
            &module,
            &enum_types,
            &aggregate_types,
            &newtype_ids,
            &layout.array_types,
            &layout.record_types,
            &layout.function_types,
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(declaration.symbol.module))
                .collect::<Vec<_>>()
        })?;
        signatures.insert(declaration.symbol, signature);
    }
    let mut externals = Vec::new();
    for binding in &bindings.imports {
        let signature = binding
            .signature
            .as_ref()
            .and_then(|signature| abstract_signature(signature, &module, &layout.record_types));
        if let Some(signature) = &signature {
            signatures.insert(binding.symbol, signature.clone());
        }
        externals.push(External {
            symbol: binding.symbol,
            signature,
        });
    }
    let mut functions = Vec::with_capacity(module.declarations.len());
    let generated_symbols = Rc::new(RefCell::new(GeneratedSymbolAllocator::new(&module)));
    let function_wrappers = module
        .declarations
        .iter()
        .map(|declaration| {
            let wrapper = generated_symbols
                .borrow_mut()
                .fresh(declaration.symbol.module);
            (declaration.symbol, wrapper)
        })
        .collect::<HashMap<_, _>>();
    let context = LoweringContext {
        module: &module,
        signatures: &signatures,
        representations: &layout.representations,
        enum_types: &enum_types,
        aggregate_types: &aggregate_types,
        newtype_ids: &newtype_ids,
        boxed_integer_type: layout.boxed_integer_type,
        boxed_number_type: layout.boxed_number_type,
        array_types: &layout.array_types,
        record_types: &layout.record_types,
        constructor_tags: &constructor_tags,
        constructors_by_type: &constructors_by_type,
        constructor_types: &layout.constructor_types,
        function_types: &layout.function_types,
        function_wrappers: &function_wrappers,
        generated_symbols,
    };
    for declaration in &module.declarations {
        let (lowered, generated) = lower_function(declaration, &context).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(declaration.symbol.module))
                .collect::<Vec<_>>()
        })?;
        functions.push(lowered);
        functions.extend(generated);
    }
    let cc = Module {
        name: module.name,
        externals,
        representations: layout.representations,
        functions,
        entry: module.entry,
        span: module.span,
    };
    verify::verify_module(&cc)?;
    Ok(BackendInput {
        cc,
        externals: bindings,
    })
}

pub(crate) fn abstract_signature(
    signature: &SourceSignature,
    module: &CoreModule,
    record_types: &HashMap<psrs_core::TypeId, ReprId>,
) -> Option<Signature> {
    Some(Signature {
        parameters: signature
            .parameters
            .iter()
            .map(|ty| scalar_source_type(ty, module, record_types))
            .collect::<Option<Vec<_>>>()?,
        result: scalar_source_type(&signature.result, module, record_types)?,
    })
}

fn scalar_source_type(
    ty: &SourceType,
    module: &CoreModule,
    record_types: &HashMap<psrs_core::TypeId, ReprId>,
) -> Option<ValueShape> {
    Some(match ty {
        SourceType::Int
        | SourceType::Char
        | SourceType::Enum { .. }
        | SourceType::String
        | SourceType::Unit => ValueShape::Integer,
        SourceType::Boolean => ValueShape::Boolean,
        SourceType::Number => ValueShape::Number,
        SourceType::Record { .. } => {
            let type_id = module
                .types
                .iter()
                .enumerate()
                .find_map(|(index, core_type)| {
                    core_type_matches_source(module, core_type, ty)
                        .then_some(psrs_core::TypeId(index as u32))
                })?;
            let representation = record_types.get(&type_id).copied()?;
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            })
        }
    })
}

pub(crate) fn signature_matches_source(
    source: &SourceSignature,
    actual: &Signature,
    representations: &RepresentationTable,
) -> bool {
    source.parameters.len() == actual.parameters.len()
        && source
            .parameters
            .iter()
            .zip(&actual.parameters)
            .all(|(source, actual)| source_shape_matches(source, actual, representations))
        && source_shape_matches(&source.result, &actual.result, representations)
}

fn source_shape_matches(
    source: &SourceType,
    actual: &ValueShape,
    representations: &RepresentationTable,
) -> bool {
    match source {
        SourceType::Int
        | SourceType::Char
        | SourceType::Enum { .. }
        | SourceType::String
        | SourceType::Unit => *actual == ValueShape::Integer,
        SourceType::Boolean => *actual == ValueShape::Boolean,
        SourceType::Number => *actual == ValueShape::Number,
        SourceType::Record { fields } => {
            let ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            }) = actual
            else {
                return false;
            };
            let Some(Representation::Product {
                fields: actual_fields,
            }) = representations.representation(*representation)
            else {
                return false;
            };
            fields.len() == actual_fields.len()
                && fields
                    .iter()
                    .zip(actual_fields)
                    .all(|((_, source), actual)| {
                        source_shape_matches(source, actual, representations)
                    })
        }
    }
}

fn core_type_matches_source(module: &CoreModule, core: &CoreType, source: &SourceType) -> bool {
    match (core, source) {
        (CoreType::I32, SourceType::Int)
        | (CoreType::Boolean, SourceType::Boolean)
        | (CoreType::F64, SourceType::Number)
        | (CoreType::Char, SourceType::Char)
        | (CoreType::String, SourceType::String)
        | (CoreType::Unit, SourceType::Unit) => true,
        (CoreType::Constructor(TypeConstructor::User(type_id)), SourceType::Enum { cases }) => {
            source_enum_cases(module, *type_id).as_ref() == Some(cases)
        }
        (CoreType::Application(function, _), SourceType::Enum { cases }) => {
            core_type_user_id(module, *function)
                .and_then(|type_id| source_enum_cases(module, type_id))
                .as_ref()
                == Some(cases)
        }
        (CoreType::Record(core_fields), SourceType::Record { fields }) => {
            core_fields.len() == fields.len()
                && core_fields.iter().zip(fields).all(
                    |((core_label, core_type), (source_label, source_type))| {
                        core_label == source_label
                            && module.types.get(core_type.0 as usize).is_some_and(|core| {
                                core_type_matches_source(module, core, source_type)
                            })
                    },
                )
        }
        _ => false,
    }
}

fn core_type_user_id(module: &CoreModule, id: psrs_core::TypeId) -> Option<HirTypeId> {
    match module.types.get(id.0 as usize)? {
        CoreType::Constructor(TypeConstructor::User(type_id)) => Some(*type_id),
        CoreType::Application(function, _) => core_type_user_id(module, *function),
        _ => None,
    }
}

fn source_enum_cases(module: &CoreModule, type_id: HirTypeId) -> Option<Vec<String>> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    if constructors.is_empty()
        || constructors.iter().enumerate().any(|(index, constructor)| {
            constructor.tag != index as u32 || constructor.field_count != 0
        })
    {
        return None;
    }
    Some(
        constructors
            .into_iter()
            .map(|constructor| constructor.name.clone())
            .collect(),
    )
}
