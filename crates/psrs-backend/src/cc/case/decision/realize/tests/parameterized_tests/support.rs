use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{
    Function, RefShape, Reference, ReprId, RepresentationTable, ValueDecl, ValueShape,
};
use psrs_core::{CaseBranch, Module};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub(super) struct LoweringContext<'a> {
    pub(super) representations: &'a RepresentationTable,
    pub(super) enum_types: &'a HashSet<HirTypeId>,
    pub(super) aggregate_types: &'a HashSet<HirTypeId>,
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    pub(super) record_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    pub(super) constructor_types: &'a HashMap<SymbolId, ReprId>,
}

pub(super) fn lower_and_verify(
    module: &Module,
    scrutinee_type: psrs_core::TypeId,
    scrutinee_shape: ValueShape,
    branches: &[CaseBranch],
    result_shape: ValueShape,
    context: &LoweringContext<'_>,
) -> Result<Function, Vec<crate::BackendError>> {
    let signatures = HashMap::new();
    let newtype_ids = module.newtype_ids.iter().copied().collect();
    let constructor_tags = module
        .constructors
        .iter()
        .map(|constructor| (constructor.symbol, constructor.tag))
        .collect();
    let mut constructors_by_type = HashMap::<HirTypeId, Vec<(SymbolId, u32)>>::new();
    for constructor in &module.constructors {
        constructors_by_type
            .entry(constructor.type_id)
            .or_default()
            .push((constructor.symbol, constructor.tag));
    }
    let function_types = HashMap::new();
    let function_wrappers = HashMap::new();
    let generated_symbols = Rc::new(RefCell::new(GeneratedSymbolAllocator::new(module)));
    let mut lowerer = FunctionLowerer {
        next_value: 1,
        values: vec![ValueDecl {
            id: crate::types::ValueId(0),
            ty: scrutinee_shape,
        }],
        locals: HashMap::new(),
        signatures: &signatures,
        representations: context.representations,
        transport_signatures: &HashMap::new(),
        module,
        source: module,
        constructor_protocols: &HashMap::new(),
        enum_types: context.enum_types,
        aggregate_types: context.aggregate_types,
        newtype_ids: &newtype_ids,
        boxed_integer_type: None,
        boxed_number_type: None,
        array_types: context.array_types,
        record_types: context.record_types,
        constructor_tags: &constructor_tags,
        constructors_by_type: &constructors_by_type,
        constructor_types: context.constructor_types,
        function_types: &function_types,
        function_wrappers: &function_wrappers,
        generated_symbols,
        owner: module.id,
        warnings: Vec::new(),
        local_types: HashMap::new(),
        generated: Vec::new(),
    };
    let mut assignments = Vec::new();
    let span = TextRange::new(0, 50);
    let result = lowerer.lower_case(
        scrutinee_type,
        crate::types::ValueId(0),
        branches,
        result_shape,
        span,
        &mut assignments,
    )?;
    let function = Function {
        symbol: SymbolId::new(module.id, 100),
        name: "projectParameterizedField".into(),
        parameters: vec![crate::types::ValueId(0)],
        values: lowerer.values,
        assignments,
        result,
        result_type: result_shape,
        span,
    };
    crate::cc::verify::verify_function(&function, &signatures, context.representations)
        .expect("realized decision DAG should verify as CC");
    Ok(function)
}

pub(super) fn reference_shape(heap: RefShape) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap,
    })
}
