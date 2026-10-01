use super::*;
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};

const SPAN: TextRange = TextRange::new(0, 1);

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let applied = TypeId(types.len() as u32);
    types.push(Type::Application(function, parameter));
    let full = TypeId(types.len() as u32);
    types.push(Type::Application(applied, result));
    full
}

fn record(types: &mut Vec<Type>, fields: Vec<(&str, TypeId)>, tail: TypeId) -> TypeId {
    let mut row = tail;
    for (label, ty) in fields.into_iter().rev() {
        let id = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: label.into(),
            ty,
            tail: row,
        });
        row = id;
    }
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    let record = TypeId(types.len() as u32);
    types.push(Type::Application(head, row));
    record
}

fn declare(index: u32, quantified: Vec<TypeVariableId>, ty: TypeId, value: Expr) -> Declaration {
    Declaration {
        symbol: SymbolId::new(ModuleId(0), index),
        name: format!("d{index}"),
        name_span: SPAN,
        quantified,
        ty,
        value,
        span: SPAN,
    }
}

fn lambda(parameter: TypeId, body: Expr, ty: TypeId) -> Expr {
    Expr {
        kind: ExprKind::Lambda {
            binder: Binder {
                id: LocalId(0),
                name: "value".into(),
                ty: parameter,
                span: SPAN,
            },
            body: Box::new(body),
        },
        ty,
        span: SPAN,
    }
}

fn integer(ty: TypeId) -> Expr {
    Expr {
        kind: ExprKind::Integer(0),
        ty,
        span: SPAN,
    }
}

fn local(ty: TypeId) -> Expr {
    Expr {
        kind: ExprKind::Local(LocalId(0)),
        ty,
        span: SPAN,
    }
}

fn packaged(types: Vec<Type>, declarations: Vec<Declaration>) -> Module {
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Rows".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations,
        entry: None,
        span: SPAN,
    }
}

struct Rows {
    int: TypeId,
    boolean: TypeId,
    variable: TypeId,
    empty: TypeId,
    types: Vec<Type>,
}

fn base() -> Rows {
    Rows {
        int: TypeId(0),
        boolean: TypeId(1),
        variable: TypeId(2),
        empty: TypeId(3),
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Boolean),
            Type::Variable(TypeVariableId(0)),
            Type::RowEmpty,
        ],
    }
}

fn use_scheme(
    rows: &mut Rows,
    scheme: TypeId,
    quantified: Vec<TypeVariableId>,
    instance: TypeId,
) -> Module {
    let definition = declare(
        1,
        quantified,
        scheme,
        lambda(
            arrow_parameter(&rows.types, scheme),
            scheme_body(rows, scheme),
            scheme,
        ),
    );
    let use_site = declare(
        0,
        Vec::new(),
        instance,
        Expr {
            kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
            ty: instance,
            span: SPAN,
        },
    );
    packaged(std::mem::take(&mut rows.types), vec![use_site, definition])
}

fn arrow_parameter(types: &[Type], arrow_id: TypeId) -> TypeId {
    crate::arrow_parts(types, arrow_id)
        .expect("scheme is an arrow")
        .0
}

fn scheme_body(rows: &Rows, scheme: TypeId) -> Expr {
    let (_, result) = crate::arrow_parts(&rows.types, scheme).expect("scheme is an arrow");
    if result == rows.int {
        integer(rows.int)
    } else {
        local(result)
    }
}

#[test]
fn closed_records_reject_extra_fields() {
    let mut rows = base();
    let closed = record(&mut rows.types, vec![("a", rows.int)], rows.empty);
    let wider = record(
        &mut rows.types,
        vec![("a", rows.int), ("b", rows.boolean)],
        rows.empty,
    );
    let scheme = arrow(&mut rows.types, closed, rows.int);
    let instance = arrow(&mut rows.types, wider, rows.int);
    let module = use_scheme(&mut rows, scheme, Vec::new(), instance);
    let errors = module
        .verify()
        .expect_err("extra closed fields are not width subtyping");
    assert!(
        errors
            .iter()
            .any(|error| error.message == "Core expression type is inconsistent with its context"),
        "{errors:?}"
    );
}

#[test]
fn flexible_row_instantiation_accepts_a_wider_argument_record() {
    let mut rows = base();
    let open = record(&mut rows.types, vec![("a", rows.int)], rows.variable);
    let wider = record(
        &mut rows.types,
        vec![("a", rows.int), ("b", rows.boolean)],
        rows.empty,
    );
    let scheme = arrow(&mut rows.types, open, rows.int);
    let instance = arrow(&mut rows.types, wider, rows.int);
    let module = use_scheme(&mut rows, scheme, vec![TypeVariableId(0)], instance);
    assert!(
        module.verify().is_ok(),
        "{:?}",
        module.verify().unwrap_err()
    );
}

#[test]
fn rigid_open_rows_reject_extra_fields() {
    let mut rows = base();
    let open = record(&mut rows.types, vec![("a", rows.int)], rows.variable);
    let wider = record(
        &mut rows.types,
        vec![("a", rows.int), ("b", rows.boolean)],
        rows.empty,
    );
    let function = arrow(&mut rows.types, open, rows.int);
    let definition = declare(
        1,
        vec![TypeVariableId(0)],
        function,
        lambda(open, integer(rows.int), function),
    );
    let argument = Expr {
        kind: ExprKind::Record {
            fields: vec![
                ("a".into(), integer(rows.int)),
                (
                    "b".into(),
                    Expr {
                        kind: ExprKind::Boolean(false),
                        ty: rows.boolean,
                        span: SPAN,
                    },
                ),
            ],
        },
        ty: wider,
        span: SPAN,
    };
    let call = Expr {
        kind: ExprKind::Application(
            Box::new(Expr {
                kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
                ty: function,
                span: SPAN,
            }),
            Box::new(argument),
        ),
        ty: rows.int,
        span: SPAN,
    };
    let main = declare(0, vec![TypeVariableId(0)], rows.int, call);
    let module = packaged(rows.types, vec![main, definition]);
    let errors = module
        .verify()
        .expect_err("a rigid row cannot absorb an extra field");
    assert!(
        errors
            .iter()
            .any(|error| error.message == "Core expression type is inconsistent with its context"),
        "{errors:?}"
    );
}

#[test]
fn row_instantiation_must_agree_at_every_use_of_the_variable() {
    let mut rows = base();
    let parameter = record(&mut rows.types, vec![("b", rows.int)], rows.variable);
    let scheme = arrow(&mut rows.types, parameter, parameter);
    let domain = record(
        &mut rows.types,
        vec![("a", rows.boolean), ("b", rows.int)],
        rows.empty,
    );
    let codomain = record(
        &mut rows.types,
        vec![("c", rows.boolean), ("b", rows.int)],
        rows.empty,
    );
    let instance = arrow(&mut rows.types, domain, codomain);
    let module = use_scheme(&mut rows, scheme, vec![TypeVariableId(0)], instance);
    let definition_only = Module {
        declarations: vec![module.declarations[1].clone()],
        ..module.clone()
    };
    assert!(
        definition_only.verify().is_ok(),
        "the open scheme is well formed: {:?}",
        definition_only.verify().unwrap_err()
    );
    let errors = module
        .verify()
        .expect_err("one row variable has two residuals");
    assert!(
        errors
            .iter()
            .any(|error| error.message == "Core expression type is inconsistent with its context"),
        "{errors:?}"
    );
}

#[test]
fn row_instantiation_may_repeat_one_residual() {
    let mut rows = base();
    let parameter = record(&mut rows.types, vec![("b", rows.int)], rows.variable);
    let scheme = arrow(&mut rows.types, parameter, parameter);
    let wide = record(
        &mut rows.types,
        vec![("a", rows.boolean), ("b", rows.int)],
        rows.empty,
    );
    let instance = arrow(&mut rows.types, wide, wide);
    let module = use_scheme(&mut rows, scheme, vec![TypeVariableId(0)], instance);
    assert!(
        module.verify().is_ok(),
        "the same residual can instantiate both sides: {:?}",
        module.verify().unwrap_err()
    );
}
