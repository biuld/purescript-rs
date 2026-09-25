use crate::cc::{AssignmentKind, RefShape, Reference, Representation, ValueShape};
use psrs_core::TypeId;
use psrs_core::{Binder, Declaration, Expr, ExprKind, Module, Type};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

#[test]
fn typed_core_generic_record_build_and_update_use_erased_storage() {
    let module = generic_record_module(false, true);
    assert!(
        module
            .declarations
            .iter()
            .any(|declaration| declaration.name == "build")
    );
    assert!(
        module
            .declarations
            .iter()
            .any(|declaration| declaration.name == "update")
    );

    let backend = crate::cc::lower_module(module)
        .expect("synthetic Typed Core generic record build and update should lower");
    let erased = erased_reference();
    let record_repr = backend
        .cc
        .representations
        .representations
        .iter()
        .position(|representation| {
            matches!(representation, Representation::Product { fields } if fields == &[ValueShape::Integer, erased])
        })
        .map(|index| crate::cc::ReprId(index as u32))
        .expect("dependent record layout should store its Array a field as erased");

    let build = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name == "build")
        .expect("the Typed Core build function must have a CC function");
    assert!(build.assignments.iter().any(|assignment| {
        matches!(
            assignment.kind,
            AssignmentKind::ProductNew { representation, ref arguments, .. }
                if representation == record_repr
                    && arguments.len() == 2
                    && value_shape(build, arguments[0]) == Some(ValueShape::Integer)
                    && value_shape(build, arguments[1]) == Some(erased)
        )
    }));

    let update = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name == "update")
        .expect("the Typed Core update function must have a CC function");
    assert!(update.assignments.iter().any(|assignment| {
        matches!(
            assignment.kind,
            AssignmentKind::ProductGet { representation, field: 0, .. }
                if representation == record_repr
        )
    }));
    assert!(update.assignments.iter().any(|assignment| {
        matches!(
            assignment.kind,
            AssignmentKind::ProductNew { representation, ref arguments, .. }
                if representation == record_repr
                    && arguments.len() == 2
                    && value_shape(update, arguments[0]) == Some(ValueShape::Integer)
                    && value_shape(update, arguments[1]) == Some(erased)
        )
    }));
}

#[test]
fn typed_core_generic_record_array_access_reports_named_backend_error() {
    let module = generic_record_module(true, false);
    assert!(
        module
            .declarations
            .iter()
            .any(|declaration| declaration.name == "read")
    );
    let expected_span = TextRange::new(24, 39);
    let errors = crate::cc::lower_module(module)
        .expect_err("dependent nominal Array a recovery should be rejected in CC");
    assert!(
        errors.iter().any(|error| {
            error.pass == "P8 closure conversion"
                && error.span == expected_span
                && error
                    .message
                    .contains("unsupported generic array field recovery")
        }),
        "expected the read function's field access to receive a source-spanned backend error; got {errors:#?}"
    );
}

fn generic_record_module(include_read: bool, include_build_update: bool) -> Module {
    let module_id = ModuleId(0);
    let variable = TypeId(0);
    let array_constructor = TypeId(1);
    let array_a = TypeId(2);
    let integer = TypeId(3);
    let record_a = TypeId(4);
    let build_type = TypeId(5);
    let update_values_type = TypeId(6);
    let update_type = TypeId(7);
    let read_type = TypeId(8);
    let types = vec![
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(psrs_core::TypeConstructor::Array),
        Type::Application(array_constructor, variable),
        Type::I32,
        Type::Record(vec![("count".into(), integer), ("values".into(), array_a)]),
        Type::Function {
            parameter: array_a,
            result: record_a,
        },
        Type::Function {
            parameter: array_a,
            result: record_a,
        },
        Type::Function {
            parameter: record_a,
            result: update_values_type,
        },
        Type::Function {
            parameter: record_a,
            result: integer,
        },
    ];
    let declarations = if include_build_update {
        vec![
            build_declaration(module_id, array_a, record_a, build_type),
            update_declaration(
                module_id,
                array_a,
                record_a,
                update_type,
                update_values_type,
            ),
        ]
    } else if include_read {
        vec![read_declaration(
            module_id, array_a, record_a, integer, read_type,
        )]
    } else {
        Vec::new()
    };
    Module {
        id: module_id,
        name: "SyntheticGenericRecord".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations,
        entry: None,
        span: TextRange::new(0, 64),
    }
}

fn build_declaration(
    module: ModuleId,
    array_a: TypeId,
    record_a: TypeId,
    function_type: TypeId,
) -> Declaration {
    let whole = TextRange::new(0, 20);
    let values = binder(0, "values", array_a, TextRange::new(6, 12));
    let body = Expr {
        kind: ExprKind::Record {
            fields: vec![
                (
                    "count".into(),
                    Expr {
                        kind: ExprKind::Integer(1),
                        ty: TypeId(3),
                        span: TextRange::new(13, 14),
                    },
                ),
                (
                    "values".into(),
                    local(values.id, array_a, TextRange::new(16, 22)),
                ),
            ],
        },
        ty: record_a,
        span: TextRange::new(13, 23),
    };
    Declaration {
        symbol: SymbolId::new(module, 0),
        name: "build".into(),
        name_span: TextRange::new(0, 5),
        quantified: vec![TypeVariableId(0)],
        ty: function_type,
        value: Expr {
            kind: ExprKind::Lambda {
                binder: values,
                body: Box::new(body),
            },
            ty: function_type,
            span: whole,
        },
        span: whole,
    }
}

fn update_declaration(
    module: ModuleId,
    array_a: TypeId,
    record_a: TypeId,
    function_type: TypeId,
    values_function_type: TypeId,
) -> Declaration {
    let whole = TextRange::new(21, 48);
    let record = binder(1, "record", record_a, TextRange::new(28, 34));
    let values = binder(2, "values", array_a, TextRange::new(35, 41));
    let body = Expr {
        kind: ExprKind::RecordUpdate {
            record: Box::new(local(record.id, record_a, TextRange::new(42, 48))),
            fields: vec![(
                "values".into(),
                local(values.id, array_a, TextRange::new(49, 55)),
            )],
        },
        ty: record_a,
        span: TextRange::new(42, 56),
    };
    let inner = Expr {
        kind: ExprKind::Lambda {
            binder: values,
            body: Box::new(body),
        },
        ty: values_function_type,
        span: whole,
    };
    Declaration {
        symbol: SymbolId::new(module, 1),
        name: "update".into(),
        name_span: TextRange::new(21, 27),
        quantified: vec![TypeVariableId(0)],
        ty: function_type,
        value: Expr {
            kind: ExprKind::Lambda {
                binder: record,
                body: Box::new(inner),
            },
            ty: function_type,
            span: whole,
        },
        span: whole,
    }
}

fn read_declaration(
    module: ModuleId,
    array_a: TypeId,
    record_a: TypeId,
    integer: TypeId,
    function_type: TypeId,
) -> Declaration {
    let whole = TextRange::new(0, 48);
    let record = binder(3, "record", record_a, TextRange::new(5, 11));
    let access_span = TextRange::new(24, 39);
    let access = Expr {
        kind: ExprKind::FieldAccess {
            record: Box::new(local(record.id, record_a, TextRange::new(24, 30))),
            field: "values".into(),
        },
        ty: array_a,
        span: access_span,
    };
    let body = Expr {
        kind: ExprKind::ArrayLength(Box::new(access)),
        ty: integer,
        span: TextRange::new(12, 40),
    };
    Declaration {
        symbol: SymbolId::new(module, 2),
        name: "read".into(),
        name_span: TextRange::new(0, 4),
        quantified: vec![TypeVariableId(0)],
        ty: function_type,
        value: Expr {
            kind: ExprKind::Lambda {
                binder: record,
                body: Box::new(body),
            },
            ty: function_type,
            span: whole,
        },
        span: whole,
    }
}

fn binder(id: u32, name: &str, ty: TypeId, span: TextRange) -> Binder {
    Binder {
        id: LocalId(id),
        name: name.into(),
        ty,
        span,
    }
}

fn local(id: LocalId, ty: TypeId, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Local(id),
        ty,
        span,
    }
}

fn value_shape(function: &crate::cc::Function, id: crate::cc::ValueId) -> Option<ValueShape> {
    function
        .values
        .iter()
        .find(|declaration| declaration.id == id)
        .map(|declaration| declaration.ty)
}

fn erased_reference() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}
