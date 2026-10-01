use super::*;
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};

const SPAN: TextRange = TextRange::new(0, 1);

fn module_with(
    id: u32,
    types: Vec<Type>,
    declarations: Vec<Declaration>,
    constructors: Vec<ConstructorInfo>,
) -> Module {
    Module {
        type_names: Vec::new(),
        id: ModuleId(id),
        name: format!("M{id}"),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors,
        declarations,
        entry: None,
        span: SPAN,
    }
}

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let applied = TypeId(types.len() as u32);
    types.push(Type::Application(function, parameter));
    let full = TypeId(types.len() as u32);
    types.push(Type::Application(applied, result));
    full
}

fn identity(module_id: u32, symbol: u32) -> Module {
    let variable = TypeVariableId(0);
    let mut types = vec![Type::Variable(variable)];
    let body = arrow(&mut types, TypeId(0), TypeId(0));
    let quantified = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable],
        body,
    });
    let value = Expr {
        kind: ExprKind::Lambda {
            binder: Binder {
                id: LocalId(0),
                name: "value".into(),
                ty: TypeId(0),
                span: SPAN,
            },
            body: Box::new(Expr {
                kind: ExprKind::Local(LocalId(0)),
                ty: TypeId(0),
                span: SPAN,
            }),
        },
        ty: quantified,
        span: SPAN,
    };
    module_with(
        module_id,
        types,
        vec![Declaration {
            symbol: SymbolId::new(ModuleId(module_id), symbol),
            name: "identity".into(),
            name_span: SPAN,
            quantified: Vec::new(),
            ty: quantified,
            value,
            span: SPAN,
        }],
        vec![ConstructorInfo {
            symbol: SymbolId::new(ModuleId(module_id), symbol + 1),
            name: "Identity".into(),
            type_id: HirTypeId::new(ModuleId(module_id), 0),
            tag: 0,
            field_count: 1,
            field_types: vec![TypeId(0)],
            parameters: vec![variable],
        }],
    )
}

fn forall_binders(module: &Module, id: TypeId) -> Vec<TypeVariableId> {
    match module.types.get(id.0 as usize) {
        Some(Type::ForAll { variables, .. }) => variables.clone(),
        other => panic!("expected a forall, found {other:?}"),
    }
}

#[test]
fn link_renumbers_type_variables_so_modules_keep_distinct_binders() {
    let linked = link(vec![identity(1, 0), identity(2, 1)]);
    let first = forall_binders(&linked, linked.declarations[0].ty);
    let second = forall_binders(&linked, linked.declarations[1].ty);
    assert_eq!(first, vec![TypeVariableId(0)]);
    assert_eq!(second, vec![TypeVariableId(1)]);
    assert_eq!(linked.constructors[1].parameters, vec![TypeVariableId(1)]);
    assert!(
        linked
            .types
            .iter()
            .any(|ty| ty == &Type::Variable(TypeVariableId(1))),
        "the second module's variable node must move with its binder"
    );
    assert!(
        linked.verify().is_ok(),
        "{:?}",
        linked.verify().unwrap_err()
    );
}

#[test]
fn link_reserves_ids_for_forall_binders_that_have_no_variable_node() {
    let unused = TypeVariableId(3);
    let mut types = vec![Type::Constructor(TypeConstructor::Int)];
    let quantified = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![unused],
        body: TypeId(0),
    });
    let holder = module_with(
        1,
        types,
        vec![Declaration {
            symbol: SymbolId::new(ModuleId(1), 0),
            name: "holder".into(),
            name_span: SPAN,
            quantified: Vec::new(),
            ty: quantified,
            value: Expr {
                kind: ExprKind::Integer(0),
                ty: TypeId(0),
                span: SPAN,
            },
            span: SPAN,
        }],
        Vec::new(),
    );
    let linked = link(vec![holder, identity(2, 1)]);
    assert_eq!(
        forall_binders(&linked, linked.declarations[0].ty),
        vec![TypeVariableId(3)]
    );
    assert_eq!(
        forall_binders(&linked, linked.declarations[1].ty),
        vec![TypeVariableId(4)]
    );
    assert!(
        linked.verify().is_ok(),
        "{:?}",
        linked.verify().unwrap_err()
    );
}
