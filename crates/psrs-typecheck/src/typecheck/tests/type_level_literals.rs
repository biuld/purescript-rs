//! Type-level literals on the shared type spine.
//!
//! A type-level string or integer is an ordinary checked type: it is decided by
//! its value, unifies with an equal literal, is what an unknown variable is
//! solved to, and survives substitution, generalization, and finalization into
//! THIR. These cases exercise each of those steps.

use super::*;

fn ty(kind: HirTypeKind, start: u32, end: u32) -> HirType {
    HirType {
        kind,
        span: TextRange::new(start, end),
    }
}

/// The `Proxy` head's stable identity. The module built by [`proxy_module`]
/// registers the declaration that owns it.
const PROXY_TYPE: hir::TypeId = hir::TypeId::new(ModuleId(0), 0);
/// The value-level constructor of `data Proxy a = Proxy`.
const PROXY_VALUE: SymbolId = SymbolId::new(ModuleId(0), 10);

/// `data Proxy a = Proxy`, so a signature naming `Proxy` has a real constructor
/// to type the `Proxy 1` and `Proxy "a"` uses with.
fn proxy_declaration() -> hir::TypeDeclaration {
    hir::TypeDeclaration {
        id: PROXY_TYPE,
        name: "Proxy".into(),
        name_span: TextRange::new(0, 5),
        kind: hir::TypeDeclarationKind::Data,
        compiler_class: None,
        parameters: vec![psrs_hir::TypeParameter {
            name: "a".into(),
            name_span: TextRange::new(6, 7),
            kind: None,
        }],
        constructors: vec![hir::Constructor {
            symbol: PROXY_VALUE,
            name: "Proxy".into(),
            name_span: TextRange::new(12, 17),
            fields: Vec::new(),
            span: TextRange::new(12, 17),
        }],
        members: Vec::new(),
        body: None,
        superclasses: Vec::new(),
        fundeps: Vec::new(),
        declared_kind: None,
        declared_roles: None,
        span: TextRange::new(0, 17),
    }
}

fn proxy_constructor_expr(start: u32) -> HirExpr {
    expr(HirExprKind::Global(PROXY_VALUE), start, start + 5)
}

fn proxy_module(values: Vec<(&str, HirType, HirExpr)>) -> hir::Module {
    let declarations = values
        .into_iter()
        .enumerate()
        .map(|(index, (name, signature, value))| {
            declaration_with_signature(index as u32, name, 80, signature, value)
        })
        .collect::<Vec<_>>();
    let mut resolved = module(declarations, false);
    resolved.types.push(proxy_declaration());
    resolved
}

/// The `Proxy` head these signatures apply. Its declaration is absent on
/// purpose: an unresolved user type elaborates to the nominal
/// `TypeConstructor::User` head, which is all a signature needs.
fn proxy(start: u32) -> HirType {
    ty(HirTypeKind::Named(PROXY_TYPE), start, start + 5)
}

fn applied(function: HirType, argument: HirType, start: u32, end: u32) -> HirType {
    ty(
        HirTypeKind::Application(Box::new(function), Box::new(argument)),
        start,
        end,
    )
}

/// `Proxy "a"`, with `start` at the head.
fn string_proxy(value: &str, start: u32) -> HirType {
    let literal_end = start + 6 + value.len() as u32 + 2;
    applied(
        proxy(start),
        ty(HirTypeKind::String(value.into()), start + 6, literal_end),
        start,
        literal_end,
    )
}

/// `Proxy <text>`, with `start` at the head and `text` the lexer's spelling of
/// the integer.
fn int_proxy(text: &str, start: u32) -> HirType {
    let literal_end = start + 6 + text.len() as u32;
    applied(
        proxy(start),
        ty(HirTypeKind::Integer(text.into()), start + 6, literal_end),
        start,
        literal_end,
    )
}

fn function(parameter: HirType, result: HirType, start: u32) -> HirType {
    let end = result.span.end;
    ty(
        HirTypeKind::Function {
            parameter: Box::new(parameter),
            result: Box::new(result),
        },
        start,
        end,
    )
}

fn forall_a(body: HirType, start: u32) -> HirType {
    let end = body.span.end;
    ty(
        HirTypeKind::Forall {
            variables: vec![psrs_hir::TypeParameter {
                name: "a".into(),
                name_span: TextRange::new(start + 7, start + 8),
                kind: None,
            }],
            body: Box::new(body),
        },
        start,
        end,
    )
}

/// `\x -> x` with a distinct binder id per call, so one module can hold
/// several of them.
fn identity_lambda(id: u32, start: u32) -> HirExpr {
    expr(
        HirExprKind::Lambda {
            binder: LocalBinder {
                id: LocalId(id),
                name: "x".into(),
                span: TextRange::new(start, start + 1),
            },
            body: Box::new(local(id, start + 3)),
        },
        start - 1,
        start + 4,
    )
}

/// `\f -> f`, forwarding the parameter's own type.
fn forwarding_lambda(id: u32, start: u32) -> HirExpr {
    expr(
        HirExprKind::Lambda {
            binder: LocalBinder {
                id: LocalId(id),
                name: "f".into(),
                span: TextRange::new(start, start + 1),
            },
            body: Box::new(local(id, start + 3)),
        },
        start - 1,
        start + 4,
    )
}

#[test]
fn elaborates_type_level_literals_into_thir() {
    // Two occurrences of the same string literal: the second must not intern a
    // second node, because equality is the value.
    let text = declaration_with_signature(
        0,
        "text",
        19,
        function(string_proxy("a", 26), string_proxy("a", 34), 26),
        identity_lambda(0, 50),
    );
    let count = declaration_with_signature(
        1,
        "count",
        55,
        function(int_proxy("1", 62), int_proxy("1", 66), 62),
        identity_lambda(1, 72),
    );
    let resolved = module(vec![text, count], false);

    let typed = typecheck_module(resolved).unwrap();
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::TypeLevelString(value) if value == "a")),
        "{:?}",
        typed.types
    );
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::TypeLevelInt(1))),
        "{:?}",
        typed.types
    );
    let strings = typed
        .types
        .iter()
        .filter(|ty| matches!(ty, Type::TypeLevelString(_)))
        .count();
    assert_eq!(strings, 1, "{:?}", typed.types);
    typed.verify().unwrap();
}

#[test]
fn reads_a_hexadecimal_type_level_integer() {
    let resolved = module(
        vec![declaration_with_signature(
            0,
            "count",
            19,
            function(int_proxy("0x1F", 26), int_proxy("0x1F", 32), 26),
            identity_lambda(0, 42),
        )],
        false,
    );

    let typed = typecheck_module(resolved).unwrap();
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::TypeLevelInt(31))),
        "{:?}",
        typed.types
    );
    typed.verify().unwrap();
}

#[test]
fn rejects_a_type_level_integer_outside_the_signed_64_bit_range() {
    let resolved = module(
        vec![declaration_with_signature(
            0,
            "count",
            19,
            function(
                int_proxy("99999999999999999999", 26),
                int_proxy("99999999999999999999", 50),
                26,
            ),
            identity_lambda(0, 80),
        )],
        false,
    );

    let errors = typecheck_module(resolved).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::UnsupportedType),
        "{errors:?}"
    );
}

#[test]
fn equal_type_level_literals_unify() {
    let resolved = module(
        vec![
            declaration_with_signature(
                0,
                "text",
                19,
                function(string_proxy("a", 26), string_proxy("a", 34), 26),
                identity_lambda(0, 50),
            ),
            declaration_with_signature(
                1,
                "count",
                55,
                function(int_proxy("7", 62), int_proxy("7", 66), 62),
                identity_lambda(1, 72),
            ),
        ],
        false,
    );

    let typed = typecheck_module(resolved).unwrap();
    typed.verify().unwrap();
}

#[test]
fn rejects_unequal_type_level_literals() {
    let resolved = module(
        vec![declaration_with_signature(
            0,
            "text",
            19,
            function(string_proxy("a", 26), string_proxy("b", 34), 26),
            identity_lambda(0, 50),
        )],
        false,
    );

    let errors = typecheck_module(resolved).unwrap_err();
    let mismatch = errors
        .iter()
        .find(|error| error.kind == TypeCheckErrorKind::TypeMismatch)
        .expect("expected a literal mismatch");
    // The literals are rendered as themselves, so the diagnostic names both
    // decided values rather than two unsolved variables.
    assert!(
        mismatch.message().contains("\"a\"") && mismatch.message().contains("\"b\""),
        "{}",
        mismatch.message()
    );
}

#[test]
fn rejects_unequal_type_level_integers() {
    let resolved = module(
        vec![declaration_with_signature(
            0,
            "count",
            19,
            function(int_proxy("7", 26), int_proxy("8", 30), 26),
            identity_lambda(0, 36),
        )],
        false,
    );

    let errors = typecheck_module(resolved).unwrap_err();
    let mismatch = errors
        .iter()
        .find(|error| error.kind == TypeCheckErrorKind::TypeMismatch)
        .expect("expected a literal mismatch");
    assert!(
        mismatch.message().contains('7') && mismatch.message().contains('8'),
        "{}",
        mismatch.message()
    );
}

/// A literal is decided, so an unknown variable is solved *to* it rather than
/// the other way round. `Proxy`'s own parameter is a fresh variable at the use
/// site, and the signature's literal decides it.
#[test]
fn solves_a_variable_to_a_type_level_literal() {
    let resolved = proxy_module(vec![
        ("one", int_proxy("1", 100), proxy_constructor_expr(100)),
        ("text", string_proxy("a", 110), proxy_constructor_expr(110)),
    ]);

    let typed = typecheck_module(resolved).unwrap();
    for (name, expected) in [
        ("one", Type::TypeLevelInt(1)),
        ("text", Type::TypeLevelString("a".into())),
    ] {
        let declaration = typed
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .expect("declaration is emitted");
        // The declaration's type is the literal itself, with no surviving
        // variable: the constructor's parameter was solved, not generalized.
        let Type::Application(_, argument) = &typed.types[declaration.ty.0 as usize] else {
            panic!("{name} should be a Proxy application: {:?}", typed.types);
        };
        assert_eq!(&typed.types[argument.0 as usize], &expected, "{name}");
    }
    assert!(
        !typed.types.iter().any(|ty| matches!(ty, Type::Variable(_))),
        "{:?}",
        typed.types
    );
    typed.verify().unwrap();
}

/// A value whose type is decided by one literal cannot be used where a
/// different literal is expected. This goes through a real declaration
/// reference, so it also exercises the scheme instance check in THIR.
#[test]
fn rejects_a_reference_at_a_different_literal() {
    let pick = declaration_with_signature(
        0,
        "pick",
        80,
        function(int_proxy("1", 88), int_proxy("1", 92), 88),
        identity_lambda(0, 96),
    );
    let use_at_two = declaration_with_signature(
        1,
        "use",
        100,
        function(int_proxy("2", 108), int_proxy("1", 112), 108),
        expr(HirExprKind::Global(SymbolId::new(ModuleId(0), 0)), 108, 112),
    );
    let mut resolved = proxy_module(Vec::new());
    resolved.declarations = vec![pick, use_at_two];

    let errors = typecheck_module(resolved).unwrap_err();
    let mismatch = errors
        .iter()
        .find(|error| error.kind == TypeCheckErrorKind::TypeMismatch)
        .expect("expected a literal mismatch");
    assert!(
        mismatch.message().contains('1') && mismatch.message().contains('2'),
        "{}",
        mismatch.message()
    );
}

/// A literal beneath a quantifier is decided, not re-instantiated: two
/// occurrences of the same literal unify, and two different ones do not.
#[test]
fn a_literal_beneath_a_quantifier_is_decided() {
    let parameter = function(string_proxy("a", 26), string_proxy("a", 34), 26);
    let accepted = forall_a(
        function(
            parameter.clone(),
            function(string_proxy("a", 50), string_proxy("a", 58), 50),
            26,
        ),
        19,
    );
    let resolved = module(
        vec![declaration_with_signature(
            0,
            "same",
            19,
            accepted,
            forwarding_lambda(0, 80),
        )],
        false,
    );
    let typed = typecheck_module(resolved).unwrap();
    typed.verify().unwrap();

    let rejected = forall_a(
        function(
            parameter,
            function(string_proxy("a", 50), string_proxy("b", 58), 50),
            26,
        ),
        19,
    );
    let resolved = module(
        vec![declaration_with_signature(
            0,
            "different",
            19,
            rejected,
            forwarding_lambda(0, 80),
        )],
        false,
    );
    let errors = typecheck_module(resolved).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "{errors:?}"
    );
}
