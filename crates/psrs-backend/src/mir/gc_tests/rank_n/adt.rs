use super::{
    app, boolean, declaration, execute_core_module, expr, global, integer, lambda, local, module,
    push_arrow, push_forall,
};
use psrs_core::{
    CaseBranch, ConstructorInfo, ExprKind, Pattern, PatternKind, Type, TypeConstructor, TypeId,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};

#[test]
fn algebraic_field_projects_a_polymorphic_closure_for_multiple_uses() {
    execute_adt_field_case(false);
}

#[test]
fn newtype_field_preserves_a_polymorphic_closure_for_multiple_uses() {
    execute_adt_field_case(true);
}

fn execute_adt_field_case(newtype: bool) {
    let int = TypeId(0);
    let bool_ty = TypeId(1);
    let variable = TypeId(2);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
    ];
    let identity_body = push_arrow(&mut types, variable, variable);
    let identity_type = push_forall(&mut types, TypeVariableId(0), identity_body);
    let int_function = push_arrow(&mut types, int, int);
    let bool_function = push_arrow(&mut types, bool_ty, bool_ty);
    let module_id = ModuleId(0);
    let identity_id = HirTypeId::new(module_id, 0);
    let constructor_symbol = SymbolId::new(module_id, 0);
    let use_symbol = SymbolId::new(module_id, 1);
    let main_symbol = SymbolId::new(module_id, 2);
    let identity_type_constructor = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::User(identity_id)));
    let use_type = push_arrow(&mut types, identity_type_constructor, int);

    let use_argument = LocalId(0);
    let field_local = LocalId(1);
    let identity_argument = LocalId(2);
    let bool_call = app(
        local(field_local, bool_function),
        boolean(true, bool_ty),
        bool_ty,
    );
    let int_call = app(local(field_local, int_function), integer(42, int), int);
    let body = expr(
        ExprKind::If {
            condition: Box::new(bool_call),
            then_branch: Box::new(int_call),
            else_branch: Box::new(integer(1, int)),
        },
        int,
    );
    let pattern = Pattern {
        kind: PatternKind::Constructor {
            symbol: constructor_symbol,
            arguments: vec![Pattern {
                kind: PatternKind::Var {
                    id: field_local,
                    ty: identity_type,
                },
                ty: identity_type,
                span: super::super::span(),
            }],
        },
        ty: identity_type_constructor,
        span: super::super::span(),
    };
    let case = expr(
        ExprKind::Case {
            scrutinee: Box::new(local(use_argument, identity_type_constructor)),
            branches: vec![CaseBranch {
                pattern,
                value: body,
                span: super::super::span(),
            }],
        },
        int,
    );
    let use_decl = declaration(
        use_symbol,
        "useIdentity",
        use_type,
        lambda(
            "boxed",
            use_argument,
            identity_type_constructor,
            case,
            use_type,
        ),
    );
    let polymorphic_identity = lambda(
        "value",
        identity_argument,
        variable,
        local(identity_argument, variable),
        identity_type,
    );
    let boxed_value = expr(
        ExprKind::Constructor {
            symbol: constructor_symbol,
            arguments: vec![polymorphic_identity],
        },
        identity_type_constructor,
    );
    let main = declaration(
        main_symbol,
        "main",
        int,
        app(global(use_symbol, use_type), boxed_value, int),
    );
    let mut core = module(types, vec![use_decl, main], main_symbol);
    if newtype {
        core.newtype_ids.push(identity_id);
    }
    core.constructors.push(ConstructorInfo {
        symbol: constructor_symbol,
        name: "Identity".into(),
        type_id: identity_id,
        tag: 0,
        field_count: 1,
        field_types: vec![identity_type],
        parameters: Vec::new(),
    });

    let context = if newtype {
        "newtype storage must preserve its polymorphic callable field"
    } else {
        "variant storage must preserve its polymorphic callable field"
    };
    execute_core_module(core, context, 42);
}
