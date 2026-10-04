use super::*;

const SPAN: TextRange = TextRange::new(0, 1);

fn expression(kind: ExprKind, ty: TypeId) -> Expr {
    Expr {
        kind,
        ty,
        span: SPAN,
    }
}

fn pattern(kind: PatternKind, ty: TypeId) -> Pattern {
    Pattern {
        kind,
        ty,
        span: SPAN,
    }
}

fn branch(pattern: Pattern, value: Expr) -> CaseBranch {
    CaseBranch {
        pattern,
        value,
        span: SPAN,
        coverage: psrs_hir::CaseBranchCoverage::Source,
    }
}

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let applied = TypeId(types.len() as u32);
    types.push(Type::Application(function, parameter));
    let arrow = TypeId(types.len() as u32);
    types.push(Type::Application(applied, result));
    arrow
}

fn case_module(
    parameter_type: TypeId,
    result_type: TypeId,
    branches: Vec<CaseBranch>,
    mut types: Vec<Type>,
) -> Module {
    let declaration_type = arrow(&mut types, parameter_type, result_type);
    let value = expression(
        ExprKind::Lambda {
            binder: Binder {
                id: LocalId(0),
                name: "input".into(),
                ty: parameter_type,
                span: SPAN,
            },
            body: Box::new(expression(
                ExprKind::Case {
                    scrutinee: Box::new(expression(ExprKind::Local(LocalId(0)), parameter_type)),
                    branches,
                },
                result_type,
            )),
        },
        declaration_type,
    );
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "PatternVerifier".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "matchValue".into(),
            name_span: SPAN,
            quantified: Vec::new(),
            ty: declaration_type,
            value,
            span: SPAN,
        }],
        entry: None,
        span: SPAN,
    }
}

fn has_error(module: &Module, message: &str) -> bool {
    module
        .verify()
        .unwrap_err()
        .iter()
        .any(|error| error.message == message)
}

#[test]
fn core_pattern_verifier_rejects_literal_and_array_element_type_mismatches() {
    let int = TypeId(0);
    let string = TypeId(1);
    let array = TypeId(2);
    let array_int = TypeId(3);
    let types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::String),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(array, int),
    ];

    let wrong_literal = case_module(
        int,
        int,
        vec![branch(
            pattern(
                PatternKind::Literal {
                    value: Literal::String("wrong".into()),
                },
                int,
            ),
            expression(ExprKind::Integer(0), int),
        )],
        types.clone(),
    );
    assert!(has_error(
        &wrong_literal,
        "Core expression type is inconsistent with its context"
    ));

    let wrong_array_element = case_module(
        array_int,
        int,
        vec![branch(
            pattern(
                PatternKind::Array {
                    elements: vec![pattern(
                        PatternKind::Literal {
                            value: Literal::String("wrong".into()),
                        },
                        string,
                    )],
                },
                array_int,
            ),
            expression(ExprKind::Integer(0), int),
        )],
        types,
    );
    assert!(has_error(
        &wrong_array_element,
        "Core expression type is inconsistent with its context"
    ));
}

#[test]
fn core_pattern_aliases_are_bound_only_in_their_own_branch_scope() {
    let int = TypeId(0);
    let aliases = case_module(
        int,
        int,
        vec![
            branch(
                pattern(
                    PatternKind::Named {
                        id: LocalId(7),
                        pattern: Box::new(pattern(PatternKind::Wildcard, int)),
                    },
                    int,
                ),
                expression(ExprKind::Local(LocalId(7)), int),
            ),
            branch(
                pattern(PatternKind::Wildcard, int),
                expression(ExprKind::Local(LocalId(7)), int),
            ),
        ],
        vec![Type::Constructor(TypeConstructor::Int)],
    );
    assert!(has_error(&aliases, "local reference is not in scope"));
}

#[test]
fn core_pattern_verifier_rejects_alias_and_nested_binder_id_collisions() {
    let int = TypeId(0);
    let duplicate = case_module(
        int,
        int,
        vec![branch(
            pattern(
                PatternKind::Named {
                    id: LocalId(7),
                    pattern: Box::new(pattern(
                        PatternKind::Var {
                            id: LocalId(7),
                            ty: int,
                        },
                        int,
                    )),
                },
                int,
            ),
            expression(ExprKind::Integer(0), int),
        )],
        vec![Type::Constructor(TypeConstructor::Int)],
    );
    assert!(has_error(
        &duplicate,
        "pattern local ID is already bound in this scope"
    ));
}

#[test]
fn core_pattern_verifier_rejects_non_finite_number_text() {
    let number = TypeId(0);
    let invalid = case_module(
        number,
        number,
        vec![branch(
            pattern(
                PatternKind::Literal {
                    value: Literal::Number("NaN".into()),
                },
                number,
            ),
            expression(ExprKind::Number("0".into()), number),
        )],
        vec![Type::Constructor(TypeConstructor::Number)],
    );
    assert!(has_error(
        &invalid,
        "number pattern literal is not a finite number"
    ));
}
