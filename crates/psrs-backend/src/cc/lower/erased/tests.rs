use super::super::super::AssignmentKind;
use psrs_core::{
    Binder, Binding, Declaration, Expr, ExprKind, Module, Type, TypeConstructor, TypeId,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

#[test]
fn a_generalized_local_binding_is_adapted_at_a_concrete_use() {
    // `let id = \x -> x in id 42`. The binding is generalized, so its runtime
    // value is erased; the concrete `id 42` use must build an erased adapter,
    // exactly as a top-level polymorphic declaration would.
    let int = TypeId(0);
    let variable = TypeId(1);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(TypeVariableId(0)),
    ];
    let polymorphic_function = push_arrow(&mut types, variable, variable);
    let int_function = push_arrow(&mut types, int, int);
    let module_id = ModuleId(0);
    let main_symbol = SymbolId::new(module_id, 0);
    let id_local = LocalId(0);
    let argument = LocalId(1);
    let identity = lambda(
        "x",
        argument,
        variable,
        local(argument, variable, 39),
        polymorphic_function,
        34,
        40,
    );
    let binding = Binding {
        binder: Binder {
            id: id_local,
            name: "id".into(),
            ty: polymorphic_function,
            span: range(29, 31),
        },
        quantified: vec![TypeVariableId(0)],
        value: identity,
        span: range(29, 40),
    };
    let use_id = expression(
        ExprKind::Application(
            Box::new(local(id_local, int_function, 44)),
            Box::new(integer(42, int, 48)),
        ),
        int,
        44,
        50,
    );
    let body = expression(
        ExprKind::Let {
            bindings: vec![binding],
            body: Box::new(use_id),
        },
        int,
        20,
        53,
    );
    let module = module(
        types,
        Declaration {
            symbol: main_symbol,
            name: "main".into(),
            name_span: range(0, 4),
            quantified: Vec::new(),
            ty: int,
            value: body,
            span: range(0, 53),
        },
        main_symbol,
    );

    let backend =
        crate::cc::lower_module(module).expect("a generalized local binding should lower to CC");
    let main = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main CC function");
    assert!(
        backend
            .cc
            .functions
            .iter()
            .any(|function| function.name.starts_with("erased_adapter_")),
        "the concrete use of the erased local value must build an erased adapter"
    );
    assert!(
        contains_indirect_call(&main.assignments),
        "the adapted local value is called indirectly through its concrete signature"
    );
    crate::mir::lower_module(backend.cc.clone())
        .expect("P9 should accept the adapted local-polymorphic module");
}

#[allow(clippy::too_many_arguments)]
fn lambda(
    name: &str,
    id: LocalId,
    parameter_type: TypeId,
    body: Expr,
    function_type: TypeId,
    start: u32,
    end: u32,
) -> Expr {
    expression(
        ExprKind::Lambda {
            binder: Binder {
                id,
                name: name.into(),
                ty: parameter_type,
                span: range(start, start + 1),
            },
            body: Box::new(body),
        },
        function_type,
        start,
        end,
    )
}

fn local(id: LocalId, ty: TypeId, start: u32) -> Expr {
    expression(ExprKind::Local(id), ty, start, start + 1)
}

fn integer(value: i32, ty: TypeId, start: u32) -> Expr {
    expression(ExprKind::Integer(value), ty, start, start + 1)
}

fn push_arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let inner = TypeId(types.len() as u32);
    types.push(Type::Application(head, parameter));
    let outer = TypeId(types.len() as u32);
    types.push(Type::Application(inner, result));
    outer
}

fn module(types: Vec<Type>, declaration: Declaration, entry: SymbolId) -> Module {
    Module {
        type_names: Vec::new(),
        id: entry.module,
        name: "Main".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![declaration],
        entry: Some(entry),
        span: range(0, 200),
    }
}

fn expression(kind: ExprKind, ty: TypeId, start: u32, end: u32) -> Expr {
    Expr {
        kind,
        ty,
        span: range(start, end),
    }
}

fn range(start: u32, end: u32) -> TextRange {
    TextRange::new(start, end)
}

fn contains_indirect_call(assignments: &[crate::cc::Assignment]) -> bool {
    assignments.iter().any(|assignment| match &assignment.kind {
        AssignmentKind::IndirectCall { .. } => true,
        AssignmentKind::If {
            then_assignments,
            else_assignments,
            ..
        } => contains_indirect_call(then_assignments) || contains_indirect_call(else_assignments),
        _ => false,
    })
}
