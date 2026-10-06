use crate::boundary::{BoundaryEvidence, RepresentationRegistry};
use crate::{BackendError, BackendInput, ExternalBindings, annotate_errors};
use psrs_core::Module as CoreModule;
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod case;
mod convert;
mod layout;
mod lower;
pub(crate) mod payload;
mod projection;
mod representation;
mod scalar;
mod source_abi;
mod verify;

pub(crate) use source_abi::abstract_signature;

use layout::{aggregate_type_ids, declaration_shape, enum_type_ids, type_layout};
use lower::{GeneratedSymbolAllocator, LoweringContext, lower_function};

pub use crate::types::ValueId;
pub use convert::{AggregateConvert, BoxKind, RecoveryEvidence, ValueConversion};
pub use projection::ExternalProjection;
pub(crate) use representation::guest_layout;
pub use representation::{
    Field, GuestCase, GuestLayout, RefShape, Reference, ReprId, Representation,
    RepresentationTable, Signature, SignatureId, ValueDecl, ValueShape, VariantCase,
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
    /// rather than a source name, per `docs/design/backend/wasm/encoding-and-structuring.md`.
    pub entry: Option<SymbolId>,
    pub span: TextRange,
}

/// A target-neutral external declaration. Platform binding metadata is kept in
/// [`ExternalBindings`] and is not part of CC identity or equality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub symbol: SymbolId,
    pub signature: Option<Signature>,
    /// The instance-aware guest projection of the external's parameters and
    /// result. It carries, for each field, the concrete guest value and the
    /// storage slot it maps to. `None` when the declaration is not a function
    /// type.
    pub projection: Option<projection::ExternalProjection>,
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
    AggregateConvert {
        destination: ValueId,
        value: ValueId,
        conversion: AggregateConvert,
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
    ArrayFill {
        destination: ValueId,
        representation: ReprId,
        length: ValueId,
        value: ValueId,
    },
    ArrayLen {
        destination: ValueId,
        value: ValueId,
    },
    /// `Array.append`: a fresh array holding `left`'s elements followed by
    /// `right`'s. Both operands and the result name the same array
    /// representation, and neither operand is mutated.
    ArrayAppend {
        destination: ValueId,
        representation: ReprId,
        left: ValueId,
        right: ValueId,
    },
    /// A source `String`'s canonical UTF-8 bytes as an `Array Int`. A source
    /// string is a sequence of Unicode scalar values, so this is lossless.
    StringToBytes {
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
    },
    /// An `Array Int` read as a source `String`. Every element must be a
    /// canonical byte and the bytes must be well-formed UTF-8.
    BytesToString {
        destination: ValueId,
        representation: ReprId,
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
    /// A multi-way choice on a closed, integer-tagged data type. Pattern
    /// matching chooses these tags; P9 carries the choice into MIR's Switch.
    TagSwitch {
        value: ValueId,
        cases: Vec<TagCase>,
        default_assignments: Vec<Assignment>,
        default_value: ValueId,
    },
    /// An explicit trap result used by impossible decision-DAG edges.
    Unreachable,
}

/// One selected arm of a tag switch, with its branch-local computations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagCase {
    pub tag: i32,
    pub assignments: Vec<Assignment>,
    pub value: ValueId,
}

/// Lowers Core with the default backend-side external binding extraction.
/// Prefer [`lower_module_with_bindings`] when the caller already owns the
/// backend input boundary.
///
/// This entry does not infer an Effect contract. A program that still contains
/// trusted Effect imports must go through [`crate::lower_cc_with_context`] or
/// [`crate::compile_with_context`] so those imports are lowered before WIT
/// linking.
pub fn lower_module(module: CoreModule) -> Result<BackendInput, Vec<BackendError>> {
    let bindings = ExternalBindings::from_core(&module);
    lower_module_with_bindings(module, bindings)
}

/// Lowers Core into target-neutral CC using bindings supplied beside CC.
pub fn lower_module_with_bindings(
    module: CoreModule,
    bindings: ExternalBindings,
) -> Result<BackendInput, Vec<BackendError>> {
    lower_module_with_relations(module, bindings, None, RepresentationRegistry::new())
}

/// Lowers Core using the Core-to-CC boundary side table: an immutable source
/// program for checked instantiation evidence and the registered representation
/// policies. `source` is the pre-lowering module when effect applications have
/// been rewritten; otherwise it is absent and relations are read from `module`.
pub(crate) fn lower_module_with_relations(
    mut module: CoreModule,
    bindings: ExternalBindings,
    source: Option<&CoreModule>,
    mut registry: RepresentationRegistry,
) -> Result<BackendInput, Vec<BackendError>> {
    crate::bindings::lower_primitives(&mut module, source)?;
    bindings.validate_core(&module)?;
    module = psrs_core::instantiate_local_rows(module, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| {
                BackendError::new("P8 local row instantiation", error.span, error.message)
                    .with_module(error.module)
            })
            .collect::<Vec<_>>()
    })?;
    let relations = source.unwrap_or(&module);
    registry.register_newtypes(relations);
    if let Err(errors) = module.verify_with_source(relations) {
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
        let signature = abstract_signature(
            binding.type_id,
            &module,
            &layout.record_types,
            &layout.array_types,
            &layout.constructor_types,
        );
        let projection = projection::project_external(
            binding.type_id,
            &module,
            &layout.record_types,
            &layout.array_types,
            &layout.constructor_types,
            &layout.representations,
        )
        .map_err(|message| {
            vec![
                BackendError::new("P8 closure conversion", module.span, message)
                    .with_module(binding.source_module),
            ]
        })?;
        if let Some(signature) = &signature {
            signatures.insert(binding.symbol, signature.clone());
        }
        externals.push(External {
            symbol: binding.symbol,
            signature,
            projection,
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
    let boundary = BoundaryEvidence::new(
        relations,
        &module,
        registry,
        layout.protocols,
        Some(layout.function_slot),
    );
    let context = LoweringContext {
        module: &module,
        boundary: &boundary,
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
    let mut warnings = Vec::new();
    for declaration in &module.declarations {
        let (lowered, generated, function_warnings) = lower_function(declaration, &context)
            .map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| error.with_module(declaration.symbol.module))
                    .collect::<Vec<_>>()
            })?;
        functions.push(lowered);
        functions.extend(generated);
        warnings.extend(function_warnings);
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
        warnings,
    })
}
