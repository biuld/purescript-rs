use super::*;
use crate::cc::RepresentationTable;
use crate::cc::lower::GeneratedSymbolAllocator;
use psrs_core::Type;
use psrs_hir::ModuleId;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[test]
fn unsupported_typed_boundary_reports_its_source_span() {
    let module = psrs_core::Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "ConversionDiagnostic".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types: vec![
            Type::Constructor(psrs_core::TypeConstructor::Int),
            Type::Constructor(psrs_core::TypeConstructor::Number),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 80),
    };
    let signatures = HashMap::new();
    let representations = RepresentationTable::default();
    let ids = HashSet::new();
    let reprs = HashMap::new();
    let tags = HashMap::new();
    let constructors = HashMap::new();
    let constructor_reprs = HashMap::new();
    let function_types = HashMap::new();
    let function_wrappers = HashMap::new();
    let mut lowerer = FunctionLowerer {
        next_value: 0,
        values: Vec::new(),
        locals: HashMap::new(),
        signatures: &signatures,
        representations: &representations,
        transport_signatures: &HashMap::new(),
        module: &module,
        source: &module,
        constructor_protocols: &HashMap::new(),
        enum_types: &ids,
        aggregate_types: &ids,
        newtype_ids: &ids,
        boxed_integer_type: None,
        boxed_number_type: None,
        array_types: &reprs,
        record_types: &reprs,
        constructor_tags: &tags,
        constructors_by_type: &constructors,
        constructor_types: &constructor_reprs,
        function_types: &function_types,
        function_wrappers: &function_wrappers,
        generated_symbols: Rc::new(RefCell::new(GeneratedSymbolAllocator::new(&module))),
        owner: module.id,
        warnings: Vec::new(),
        local_types: HashMap::new(),
        generated: Vec::new(),
    };
    let span = TextRange::new(30, 45);
    let errors = lowerer
        .plan_conversion(
            TypeId(0),
            TypeId(1),
            ValueShape::Integer,
            ValueShape::Number,
            span,
            None,
        )
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 closure conversion"
                && error.span == span
                && error.message.contains("unsupported aggregate conversion"))
    );

    // An abstract aggregate value and the concrete representation of the
    // same declaration are related by a reference cast (DEC-13).
    let aggregate = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    });
    let representation = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(ReprId(3)),
    });
    assert!(matches!(
        lowerer.plan_conversion(TypeId(0), TypeId(1), aggregate, representation, span, None),
        Ok(ValueConversion::RecoverReference { .. })
    ));
    assert!(matches!(
        lowerer
            .plan_conversion(TypeId(0), TypeId(1), representation, aggregate, span, None)
            .expect("a representation should recover its aggregate supertype"),
        ValueConversion::RecoverReference { .. }
    ));
}
