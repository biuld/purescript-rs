use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{
    AssignmentKind, Function, RefShape, Reference, ReprId, Representation, RepresentationTable,
    ValueDecl, ValueShape, VariantCase,
};
use psrs_core::{
    CaseBranch, ConstructorInfo, Expr, ExprKind, Module, Pattern, PatternKind, Type,
    TypeConstructor,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[test]
fn parameterized_array_field_recovery_uses_the_canonical_generic_array() {
    let module_id = ModuleId(0);
    let wrap_type = HirTypeId::new(module_id, 0);
    let wrap = SymbolId::new(module_id, 0);
    let array_a = psrs_core::TypeId(3);
    let wrap_a = psrs_core::TypeId(4);
    let local = LocalId(77);
    let module = Module {
        id: module_id,
        name: "ParameterizedArrayDecisionTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(wrap_type)),
            Type::Variable(TypeVariableId(0)),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(psrs_core::TypeId(2), psrs_core::TypeId(1)),
            Type::Application(psrs_core::TypeId(0), psrs_core::TypeId(1)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![ConstructorInfo {
            symbol: wrap,
            name: "Wrap".into(),
            type_id: wrap_type,
            tag: 0,
            field_count: 1,
            field_types: vec![array_a],
        }],
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 40),
    };
    let pattern = Pattern {
        kind: PatternKind::Constructor {
            symbol: wrap,
            arguments: vec![Pattern {
                kind: PatternKind::Var {
                    id: local,
                    ty: array_a,
                },
                ty: array_a,
                span: TextRange::new(7, 13),
            }],
        },
        ty: wrap_a,
        span: TextRange::new(1, 14),
    };
    let branches = vec![CaseBranch {
        pattern,
        value: Expr {
            kind: ExprKind::Local(local),
            ty: array_a,
            span: TextRange::new(18, 24),
        },
        span: TextRange::new(1, 24),
    }];
    let mut representations = RepresentationTable::default();
    let array_repr = representations.reserve();
    let wrap_repr = representations.reserve();
    let array_shape = reference_shape(RefShape::Repr(array_repr));
    representations.set(
        array_repr,
        Representation::Array {
            element: reference_shape(RefShape::Erased),
        },
    );
    representations.set(
        wrap_repr,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![reference_shape(RefShape::Erased)],
            }],
        },
    );
    let enum_types = HashSet::new();
    let aggregate_types = HashSet::from([wrap_type]);
    let array_types = HashMap::from([(array_a, array_repr)]);
    let record_types = HashMap::new();
    let constructor_types = HashMap::from([(wrap, wrap_repr)]);
    let context = LoweringContext {
        representations: &representations,
        enum_types: &enum_types,
        aggregate_types: &aggregate_types,
        array_types: &array_types,
        record_types: &record_types,
        constructor_types: &constructor_types,
    };
    let function = lower_and_verify(
        &module,
        wrap_a,
        reference_shape(RefShape::Aggregate),
        &branches,
        array_shape,
        &context,
    )
    .expect("generic array fields should recover through their canonical layout");
    assert_eq!(function.result_type, array_shape);
    assert!(
        function.assignments.iter().any(|assignment| matches!(
            assignment.kind,
            AssignmentKind::VariantGet { field: 0, .. }
        ))
    );
}

#[test]
fn nested_parameterized_array_projection_recovers_each_canonical_boundary() {
    let module_id = ModuleId(0);
    let inner_type = HirTypeId::new(module_id, 0);
    let outer_type = HirTypeId::new(module_id, 1);
    let inner = SymbolId::new(module_id, 0);
    let outer = SymbolId::new(module_id, 1);
    let array_a = psrs_core::TypeId(4);
    let inner_a = psrs_core::TypeId(5);
    let outer_a = psrs_core::TypeId(6);
    let local = LocalId(81);
    let module = Module {
        id: module_id,
        name: "NestedParameterizedDecisionTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(inner_type)),
            Type::Constructor(TypeConstructor::User(outer_type)),
            Type::Variable(TypeVariableId(0)),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(psrs_core::TypeId(3), psrs_core::TypeId(2)),
            Type::Application(psrs_core::TypeId(0), psrs_core::TypeId(2)),
            Type::Application(psrs_core::TypeId(1), psrs_core::TypeId(2)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![
            ConstructorInfo {
                symbol: inner,
                name: "Inner".into(),
                type_id: inner_type,
                tag: 0,
                field_count: 1,
                field_types: vec![array_a],
            },
            ConstructorInfo {
                symbol: outer,
                name: "Outer".into(),
                type_id: outer_type,
                tag: 0,
                field_count: 1,
                field_types: vec![inner_a],
            },
        ],
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 60),
    };
    let pattern = Pattern {
        kind: PatternKind::Constructor {
            symbol: outer,
            arguments: vec![Pattern {
                kind: PatternKind::Constructor {
                    symbol: inner,
                    arguments: vec![Pattern {
                        kind: PatternKind::Var {
                            id: local,
                            ty: array_a,
                        },
                        ty: array_a,
                        span: TextRange::new(13, 20),
                    }],
                },
                ty: inner_a,
                span: TextRange::new(7, 21),
            }],
        },
        ty: outer_a,
        span: TextRange::new(1, 22),
    };
    let branches = vec![CaseBranch {
        pattern,
        value: Expr {
            kind: ExprKind::Local(local),
            ty: array_a,
            span: TextRange::new(26, 32),
        },
        span: TextRange::new(1, 32),
    }];
    let mut representations = RepresentationTable::default();
    let array_repr = representations.reserve();
    let inner_repr = representations.reserve();
    let outer_repr = representations.reserve();
    let array_shape = reference_shape(RefShape::Repr(array_repr));
    representations.set(
        array_repr,
        Representation::Array {
            element: reference_shape(RefShape::Erased),
        },
    );
    representations.set(
        inner_repr,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![reference_shape(RefShape::Erased)],
            }],
        },
    );
    representations.set(
        outer_repr,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![reference_shape(RefShape::Erased)],
            }],
        },
    );
    let enum_types = HashSet::new();
    let aggregate_types = HashSet::from([inner_type, outer_type]);
    let array_types = HashMap::from([(array_a, array_repr)]);
    let record_types = HashMap::new();
    let constructor_types = HashMap::from([(inner, inner_repr), (outer, outer_repr)]);
    let context = LoweringContext {
        representations: &representations,
        enum_types: &enum_types,
        aggregate_types: &aggregate_types,
        array_types: &array_types,
        record_types: &record_types,
        constructor_types: &constructor_types,
    };
    let function = lower_and_verify(
        &module,
        outer_a,
        reference_shape(RefShape::Aggregate),
        &branches,
        array_shape,
        &context,
    )
    .expect("nested generic array fields should recover through canonical layouts");
    assert_eq!(function.result_type, array_shape);
    assert_eq!(
        function
            .assignments
            .iter()
            .filter(|assignment| matches!(assignment.kind, AssignmentKind::VariantGet { .. }))
            .count(),
        2
    );
}

#[test]
fn generic_record_pattern_projects_its_canonical_array_field() {
    let module_id = ModuleId(0);
    let array_a = psrs_core::TypeId(2);
    let record_a = psrs_core::TypeId(3);
    let local = LocalId(88);
    let module = Module {
        id: module_id,
        name: "GenericRecordDecisionTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Variable(TypeVariableId(0)),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(psrs_core::TypeId(1), psrs_core::TypeId(0)),
            Type::Record(vec![("values".into(), array_a)]),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 40),
    };
    let pattern = Pattern {
        kind: PatternKind::Record {
            fields: vec![(
                "values".into(),
                Pattern {
                    kind: PatternKind::Var {
                        id: local,
                        ty: array_a,
                    },
                    ty: array_a,
                    span: TextRange::new(12, 18),
                },
            )],
        },
        ty: record_a,
        span: TextRange::new(1, 19),
    };
    let branches = vec![CaseBranch {
        pattern,
        value: Expr {
            kind: ExprKind::Local(local),
            ty: array_a,
            span: TextRange::new(23, 29),
        },
        span: TextRange::new(1, 29),
    }];
    let mut representations = RepresentationTable::default();
    let array_repr = representations.reserve();
    let record_repr = representations.reserve();
    let array_shape = reference_shape(RefShape::Repr(array_repr));
    representations.set(
        array_repr,
        Representation::Array {
            element: reference_shape(RefShape::Erased),
        },
    );
    representations.set(
        record_repr,
        Representation::Product {
            fields: vec![array_shape],
        },
    );
    representations.set_product_labels(record_repr, vec!["values".into()]);
    let enum_types = HashSet::new();
    let aggregate_types = HashSet::new();
    let array_types = HashMap::from([(array_a, array_repr)]);
    let record_types = HashMap::from([(record_a, record_repr)]);
    let constructor_types = HashMap::new();
    let context = LoweringContext {
        representations: &representations,
        enum_types: &enum_types,
        aggregate_types: &aggregate_types,
        array_types: &array_types,
        record_types: &record_types,
        constructor_types: &constructor_types,
    };
    let function = lower_and_verify(
        &module,
        record_a,
        reference_shape(RefShape::Repr(record_repr)),
        &branches,
        array_shape,
        &context,
    )
    .expect("generic record fields should project using the canonical array layout");
    assert_eq!(function.result_type, array_shape);
    assert!(
        function.assignments.iter().any(|assignment| matches!(
            assignment.kind,
            AssignmentKind::ProductGet { field: 0, .. }
        ))
    );
}

struct LoweringContext<'a> {
    representations: &'a RepresentationTable,
    enum_types: &'a HashSet<HirTypeId>,
    aggregate_types: &'a HashSet<HirTypeId>,
    array_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    record_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    constructor_types: &'a HashMap<SymbolId, ReprId>,
}

fn lower_and_verify(
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
        module,
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
        erased_function_types: HashMap::new(),
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

fn reference_shape(heap: RefShape) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap,
    })
}
