use super::super::super::{AssignmentKind, Function, ValueId, ValueShape};
use psrs_core::{Binder, Binding, Declaration, Expr, ExprKind, Module, Type, TypeId};
use psrs_hir::{Intrinsic, LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;

#[test]
fn mutually_recursive_local_functions_lower_to_explicit_closures() {
    let backend = crate::cc::lower_module(mutual_recursion_module())
        .expect("mutually recursive local functions should lower to CC");
    let recursive = backend
        .cc
        .functions
        .iter()
        .filter(|function| function.name.contains("_letrec_"))
        .collect::<Vec<_>>();
    assert_eq!(recursive.len(), 2);
    let recursive_symbols = recursive
        .iter()
        .map(|function| function.symbol)
        .collect::<std::collections::HashSet<_>>();
    for function in recursive {
        let references = function
            .assignments
            .iter()
            .filter_map(|assignment| match assignment.kind {
                AssignmentKind::FunctionRef { function, .. } => Some(function),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(references, recursive_symbols);
        assert!(contains_indirect_call(&function.assignments));
        assert!(
            function
                .assignments
                .iter()
                .all(|assignment| assignment.span.start < assignment.span.end)
        );
    }
    crate::mir::lower_module(backend.cc.clone())
        .expect("P9 should accept the recursive closure-converted CC module");
}

fn mutual_recursion_module() -> Module {
    let int = TypeId(0);
    let boolean = TypeId(1);
    let mut types = vec![
        Type::Constructor(psrs_core::TypeConstructor::Int),
        Type::Constructor(psrs_core::TypeConstructor::Boolean),
    ];
    let int_function = push_arrow(&mut types, int, int);
    let module_id = ModuleId(0);
    let main_symbol = SymbolId::new(module_id, 0);
    let f_id = LocalId(1);
    let g_id = LocalId(2);
    let f_arg = LocalId(3);
    let g_arg = LocalId(4);
    let f_binding = binding(
        "f",
        f_id,
        int_function,
        lambda(
            "n",
            f_arg,
            int,
            recursive_if(f_arg, g_id, 0, int, boolean, int_function, 41),
            int_function,
            40,
            65,
        ),
        30,
        66,
    );
    let g_binding = binding(
        "g",
        g_id,
        int_function,
        lambda(
            "n",
            g_arg,
            int,
            recursive_if(g_arg, f_id, 1, int, boolean, int_function, 81),
            int_function,
            80,
            105,
        ),
        70,
        106,
    );
    let call = expression(
        ExprKind::Application(
            Box::new(local(f_id, int_function, 120)),
            Box::new(integer(5, int, 121)),
        ),
        int,
        120,
        122,
    );
    let body = expression(
        ExprKind::Let {
            bindings: vec![f_binding, g_binding],
            body: Box::new(call),
        },
        int,
        20,
        125,
    );
    module(
        types,
        Declaration {
            symbol: main_symbol,
            name: "main".into(),
            name_span: range(0, 4),
            quantified: Vec::new(),
            ty: int,
            value: body,
            span: range(0, 125),
        },
        main_symbol,
    )
}

fn escaping_recursive_module() -> Module {
    let int = TypeId(0);
    let boolean = TypeId(1);
    let mut types = vec![
        Type::Constructor(psrs_core::TypeConstructor::Int),
        Type::Constructor(psrs_core::TypeConstructor::Boolean),
    ];
    let int_function = push_arrow(&mut types, int, int);
    let module_id = ModuleId(0);
    let main_symbol = SymbolId::new(module_id, 0);
    let loop_id = LocalId(10);
    let seed_id = LocalId(11);
    let loop_arg = LocalId(12);
    let escaped_arg = LocalId(13);
    let seed = binding("seed", seed_id, int, integer(17, int, 22), 20, 23);
    let recursive = binding(
        "loop",
        loop_id,
        int_function,
        lambda(
            "n",
            loop_arg,
            int,
            expression(
                ExprKind::If {
                    condition: Box::new(eq_zero(loop_arg, int, boolean, 50)),
                    then_branch: Box::new(local(seed_id, int, 56)),
                    else_branch: Box::new(call_recursive(loop_id, loop_arg, int_function, int, 64)),
                },
                int,
                50,
                88,
            ),
            int_function,
            40,
            90,
        ),
        30,
        92,
    );
    let escaping = lambda(
        "x",
        escaped_arg,
        int,
        expression(
            ExprKind::Application(
                Box::new(local(loop_id, int_function, 115)),
                Box::new(local(escaped_arg, int, 116)),
            ),
            int,
            115,
            118,
        ),
        int_function,
        100,
        124,
    );
    let let_expression = expression(
        ExprKind::Let {
            bindings: vec![seed, recursive],
            body: Box::new(escaping),
        },
        int_function,
        19,
        125,
    );
    module(
        types,
        Declaration {
            symbol: main_symbol,
            name: "main".into(),
            name_span: range(0, 4),
            quantified: Vec::new(),
            ty: int_function,
            value: let_expression,
            span: range(0, 125),
        },
        main_symbol,
    )
}

#[allow(clippy::too_many_arguments)]
fn recursive_if(
    parameter: LocalId,
    target: LocalId,
    base: i32,
    int: TypeId,
    boolean: TypeId,
    function_type: TypeId,
    start: u32,
) -> Expr {
    expression(
        ExprKind::If {
            condition: Box::new(eq_zero(parameter, int, boolean, start)),
            then_branch: Box::new(integer(base, int, start + 6)),
            else_branch: Box::new(call_recursive(
                target,
                parameter,
                function_type,
                int,
                start + 10,
            )),
        },
        int,
        start,
        start + 25,
    )
}

fn call_recursive(
    target: LocalId,
    parameter: LocalId,
    function_type: TypeId,
    int: TypeId,
    start: u32,
) -> Expr {
    let decremented = expression(
        ExprKind::IntrinsicCall {
            intrinsic: Intrinsic::IntSub,
            arguments: vec![local(parameter, int, start + 1), integer(1, int, start + 2)],
        },
        int,
        start + 1,
        start + 3,
    );
    expression(
        ExprKind::Application(
            Box::new(local(target, function_type, start)),
            Box::new(decremented),
        ),
        int,
        start,
        start + 4,
    )
}

fn eq_zero(local_id: LocalId, int: TypeId, boolean: TypeId, start: u32) -> Expr {
    expression(
        ExprKind::IntrinsicCall {
            intrinsic: Intrinsic::IntEq,
            arguments: vec![local(local_id, int, start), integer(0, int, start + 1)],
        },
        boolean,
        start,
        start + 2,
    )
}

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

fn binding(name: &str, id: LocalId, ty: TypeId, value: Expr, start: u32, end: u32) -> Binding {
    Binding {
        binder: Binder {
            id,
            name: name.into(),
            ty,
            span: range(start, start + 1),
        },
        quantified: Vec::new(),
        value,
        span: range(start, end),
    }
}

fn local(id: LocalId, ty: TypeId, start: u32) -> Expr {
    expression(ExprKind::Local(id), ty, start, start + 1)
}

fn integer(value: i32, ty: TypeId, start: u32) -> Expr {
    expression(ExprKind::Integer(value), ty, start, start + 1)
}

fn push_arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(psrs_core::TypeConstructor::Function));
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
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        ordinary_data_ids: Vec::new(),
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

fn value_shape(function: &Function, value: ValueId) -> Option<ValueShape> {
    function
        .values
        .iter()
        .find(|declaration| declaration.id == value)
        .map(|declaration| declaration.ty)
}

mod initialization;

mod escaping;
