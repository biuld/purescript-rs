use super::super::super::{AssignmentKind, Function, ValueId, ValueShape};
use psrs_core::{Binder, Binding, Declaration, Expr, ExprKind, Module, Primitive, Type, TypeId};
use psrs_hir::{LocalId, ModuleId, SymbolId};
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

#[test]
fn recursive_function_captures_an_earlier_same_let_value_and_escapes() {
    let backend = crate::cc::lower_module(escaping_recursive_module())
        .expect("a closure may escape while capturing a recursive local function");
    let main = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main CC function");
    let recursive = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name.starts_with("loop_letrec_"))
        .expect("lifted recursive function");
    let escaped = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name.starts_with("lambda_"))
        .expect("lifted escaping closure");

    let seed_value = main
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::Constant(17) => {
                assert_eq!(assignment.span, TextRange::new(22, 23));
                Some(assignment.destination)
            }
            _ => None,
        })
        .expect("main evaluates the same-Let seed before the recursive function");
    let recursive_value = main
        .assignments
        .iter()
        .find_map(|assignment| match &assignment.kind {
            AssignmentKind::FunctionRef {
                function, captures, ..
            } if *function == recursive.symbol => {
                assert_eq!(captures, &[seed_value]);
                assert_eq!(assignment.span, TextRange::new(30, 92));
                Some(assignment.destination)
            }
            _ => None,
        })
        .expect("main creates the recursive function value");
    let seed_index = main
        .assignments
        .iter()
        .position(|assignment| matches!(assignment.kind, AssignmentKind::Constant(17)))
        .expect("seed assignment exists");
    let recursive_index = main
        .assignments
        .iter()
        .position(|assignment| {
            matches!(
                assignment.kind,
                AssignmentKind::FunctionRef { function, .. } if function == recursive.symbol
            )
        })
        .expect("recursive function reference exists");
    assert!(seed_index < recursive_index);
    let escaping_reference = main
        .assignments
        .iter()
        .find(|assignment| {
            matches!(
                assignment.kind,
                AssignmentKind::FunctionRef { function, .. } if function == escaped.symbol
            )
        })
        .expect("main creates the escaping closure");
    let AssignmentKind::FunctionRef { captures, .. } = &escaping_reference.kind else {
        unreachable!("the selected assignment is a function reference");
    };
    assert_eq!(captures, &[recursive_value]);
    assert_eq!(escaping_reference.span, TextRange::new(100, 124));
    assert!(escaped.assignments.iter().any(|assignment| {
        matches!(
            assignment.kind,
            AssignmentKind::ClosureGetCapture { index: 0, .. }
        )
    }));
    assert!(contains_indirect_call(&escaped.assignments));
    let escaped_capture = escaped
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::ClosureGetCapture { index: 0, .. } => Some(assignment.destination),
            _ => None,
        })
        .expect("escaping closure restores the captured recursive function");
    assert_eq!(
        value_shape(escaped, escaped_capture),
        value_shape(main, recursive_value)
    );

    let captured_seed = recursive
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::ClosureGetCapture { index: 0, .. } => Some(assignment.destination),
            _ => None,
        })
        .expect("recursive closure restores its same-Let seed capture");
    assert!(recursive.assignments.iter().any(|assignment| {
        matches!(
            &assignment.kind,
            AssignmentKind::FunctionRef { function, captures, .. }
                if *function == recursive.symbol && captures == &[captured_seed]
        ) && assignment.span == TextRange::new(30, 92)
    }));
    assert_eq!(recursive.span, TextRange::new(40, 90));
    assert_eq!(
        value_shape(recursive, captured_seed),
        Some(ValueShape::Integer)
    );
    crate::mir::lower_module(backend.cc.clone())
        .expect("P9 should accept the escaping recursive closure module");
}

fn mutual_recursion_module() -> Module {
    let int = TypeId(0);
    let boolean = TypeId(1);
    let int_function = TypeId(2);
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
            recursive_if(f_arg, g_id, 0, int, boolean, 41),
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
            recursive_if(g_arg, f_id, 1, int, boolean, 81),
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
        vec![Type::I32, Type::Boolean, function_type(int, int)],
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
    let int_function = TypeId(2);
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
        vec![Type::I32, Type::Boolean, function_type(int, int)],
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

fn recursive_if(
    parameter: LocalId,
    target: LocalId,
    base: i32,
    int: TypeId,
    boolean: TypeId,
    start: u32,
) -> Expr {
    expression(
        ExprKind::If {
            condition: Box::new(eq_zero(parameter, int, boolean, start)),
            then_branch: Box::new(integer(base, int, start + 6)),
            else_branch: Box::new(call_recursive(
                target,
                parameter,
                TypeId(2),
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
        ExprKind::Primitive {
            op: Primitive::IntSub,
            left: Box::new(local(parameter, int, start + 1)),
            right: Box::new(integer(1, int, start + 2)),
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
        ExprKind::Primitive {
            op: Primitive::IntEq,
            left: Box::new(local(local_id, int, start)),
            right: Box::new(integer(0, int, start + 1)),
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

fn function_type(parameter: TypeId, result: TypeId) -> Type {
    Type::Function { parameter, result }
}

fn module(types: Vec<Type>, declaration: Declaration, entry: SymbolId) -> Module {
    Module {
        id: entry.module,
        name: "Main".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
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
