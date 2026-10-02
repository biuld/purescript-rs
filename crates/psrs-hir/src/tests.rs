use super::*;

fn class_type(module: ModuleId, index: u32) -> TypeDeclaration {
    TypeDeclaration {
        id: TypeId::new(module, index),
        name: format!("Class{index}"),
        name_span: TextRange::default(),
        kind: TypeDeclarationKind::Class,
        parameters: Vec::new(),
        constructors: Vec::new(),
        members: Vec::new(),
        body: None,
        superclasses: Vec::new(),
        fundeps: Vec::new(),
        declared_kind: None,
        declared_roles: None,
        span: TextRange::default(),
    }
}

#[test]
fn type_substitution_renames_forall_binders_to_preserve_free_replacements() {
    let span = TextRange::default();
    let source = Type {
        kind: TypeKind::Forall {
            variables: vec![TypeParameter {
                name: "b".into(),
                name_span: span,
                kind: None,
            }],
            body: Box::new(Type {
                kind: TypeKind::Function {
                    parameter: Box::new(Type {
                        kind: TypeKind::Variable("b".into()),
                        span,
                    }),
                    result: Box::new(Type {
                        kind: TypeKind::Variable("a".into()),
                        span,
                    }),
                },
                span,
            }),
        },
        span,
    };
    let replacement = Type {
        kind: TypeKind::Variable("b".into()),
        span,
    };
    let substitution_name = "__psrs_type_subst_b_0";
    let substitutions = HashMap::from([
        ("a".into(), replacement),
        (
            substitution_name.into(),
            Type {
                kind: TypeKind::Constructor(BuiltinType::Int),
                span,
            },
        ),
    ]);
    let mut next_fresh = 0;
    let substituted = substitute_type_variables(&source, &substitutions, &mut next_fresh);

    let TypeKind::Forall { variables, body } = substituted.kind else {
        panic!("substitution must preserve the forall");
    };
    let renamed_binder = &variables[0].name;
    assert_ne!(
        renamed_binder, "b",
        "the replacement's free variable was captured"
    );
    assert_ne!(
        renamed_binder, substitution_name,
        "the fresh name collided with a substitution key"
    );
    let TypeKind::Function { parameter, result } = body.kind else {
        panic!("substitution must preserve the function body");
    };
    assert_eq!(parameter.kind, TypeKind::Variable(renamed_binder.clone()));
    assert_eq!(result.kind, TypeKind::Variable("b".into()));
}

fn chain_instance(
    module: ModuleId,
    symbol_index: u32,
    class_index: u32,
    chain_id: u32,
    chain_position: u32,
) -> InstanceDeclaration {
    InstanceDeclaration {
        symbol: SymbolId::new(module, symbol_index),
        name: format!("instance{symbol_index}"),
        name_span: TextRange::default(),
        class_id: TypeId::new(module, class_index),
        chain_id,
        chain_position,
        context: Vec::new(),
        head: Type {
            kind: TypeKind::Constructor(BuiltinType::Int),
            span: TextRange::default(),
        },
        members: Vec::new(),
        derivation: None,
        span: TextRange::default(),
    }
}

fn module_with_instances(instances: Vec<InstanceDeclaration>) -> Module {
    let module = ModuleId(0);
    Module {
        id: module,
        name: "Main".into(),
        externals: Vec::new(),
        imports: Vec::new(),
        exports: None,
        declarations: Vec::new(),
        types: vec![class_type(module, 0), class_type(module, 1)],
        instances,
        fixities: Vec::new(),
        span: TextRange::default(),
    }
}

fn assert_chain_error(instances: Vec<InstanceDeclaration>, expected: &'static str) {
    let errors = module_with_instances(instances)
        .verify()
        .expect_err("malformed instance chains must be rejected");
    assert!(
        errors.iter().any(|error| error.message == expected),
        "expected {expected:?}, got {errors:?}"
    );
}

#[test]
fn verifier_rejects_references_to_out_of_scope_locals() {
    let module_id = ModuleId(0);
    let module = Module {
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        imports: Vec::new(),
        exports: None,
        declarations: vec![Declaration {
            symbol: SymbolId::new(module_id, 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            value: Expr {
                kind: ExprKind::Local(LocalId(9)),
                span: TextRange::new(7, 8),
            },
            signature: None,
            span: TextRange::new(0, 8),
        }],
        types: Vec::new(),
        instances: Vec::new(),
        fixities: Vec::new(),
        span: TextRange::new(0, 8),
    };

    let errors = module.verify().unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "local reference is not in scope");
}

#[test]
fn verifier_rejects_instance_chain_position_gaps_and_reordering() {
    let module = ModuleId(0);
    assert_chain_error(
        vec![
            chain_instance(module, 0, 0, 1, 0),
            chain_instance(module, 1, 0, 1, 2),
        ],
        "instance chain positions must be contiguous and ordered",
    );
    assert_chain_error(
        vec![
            chain_instance(module, 0, 0, 1, 1),
            chain_instance(module, 1, 0, 1, 0),
        ],
        "an instance chain must begin at position zero",
    );
}

#[test]
fn verifier_rejects_reopened_chains_and_class_changes() {
    let module = ModuleId(0);
    assert_chain_error(
        vec![
            chain_instance(module, 0, 0, 1, 0),
            chain_instance(module, 1, 0, 2, 0),
            chain_instance(module, 2, 0, 1, 1),
        ],
        "instance-chain branches must remain contiguous",
    );
    assert_chain_error(
        vec![
            chain_instance(module, 0, 0, 1, 0),
            chain_instance(module, 1, 1, 1, 1),
        ],
        "one instance chain contains different classes",
    );
}
