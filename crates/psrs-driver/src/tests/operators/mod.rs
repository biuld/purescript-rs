mod opaque_type;
mod prim_type;

use super::*;

const FIXITY_SOURCE: &str = "module Main where\n\
    infixr 4 subtract as <+>\n\
    subtract x y = x - y\n\
    main = 10 <+> 3 <+> 2\n";

#[test]
fn unresolved_ast_keeps_operator_chains_and_sections() {
    let module = lower_source_to_ast("Main.purs", FIXITY_SOURCE).unwrap();
    assert_eq!(module.fixities.len(), 1);
    assert_eq!(module.fixities[0].target.text, "subtract");
    assert_eq!(module.fixities[0].operator.text, "<+>");
    assert_eq!(module.fixities[0].precedence, 4);

    let main = module
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "main")
        .unwrap();
    let psrs_ast::ExprKind::OperatorChain {
        operands,
        operators,
    } = &main.value.kind
    else {
        panic!("P2 must retain an unresolved operator chain");
    };
    assert_eq!(operands.len(), 3);
    assert_eq!(operators.len(), 2);
    assert!(operators.iter().all(|operator| operator.name.text == "<+>"));

    let section = lower_source_to_ast(
        "Main.purs",
        "module Main where\ninfixr 4 subtract as <+>\nsubtract x y = x - y\nmain = (10 <+> _) 3\n",
    )
    .unwrap();
    let main = section
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "main")
        .unwrap();
    let psrs_ast::ExprKind::Application(function, _) = &main.value.kind else {
        panic!("expected section application");
    };
    assert!(matches!(
        function.kind,
        psrs_ast::ExprKind::OperatorSection { .. }
    ));
}

#[test]
fn resolution_binds_operator_alias_identity_and_fixity_before_desugaring() {
    let modules = resolve_program_sources(&[("Main.purs", FIXITY_SOURCE)]).unwrap();
    let main = modules[0]
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .unwrap();
    let psrs_hir::ExprKind::OperatorChain {
        operators,
        operands,
    } = &main.value.kind
    else {
        panic!("P3 must preserve the resolved operator chain for P4");
    };
    let fixity = modules[0]
        .fixities
        .iter()
        .find(|fixity| fixity.operator == "<+>")
        .unwrap();
    assert_eq!(fixity.target_name, "subtract");
    assert_eq!(fixity.precedence, 4);
    assert_eq!(fixity.associativity, psrs_hir::Associativity::Right);
    assert!(operators.iter().all(|operator| {
        operator.symbol
            == match fixity.target {
                psrs_hir::FixityTarget::Value(symbol) => symbol,
                psrs_hir::FixityTarget::Type(_) => panic!("value operator has a type target"),
            }
            && operator.precedence == 4
            && operator.associativity == psrs_hir::Associativity::Right
    }));
    assert_eq!(operands.len(), 3);
}

#[test]
fn imported_type_operator_chains_keep_fixity_until_p4() {
    let library = (
        "Lib.purs",
        "module Lib (Tuple(..), type (:*:), Natural, type (~>)) where\n\
            data Tuple a b = Tuple a b\n\
            infixr 6 type Tuple as :*: \n\
            type Natural f g = forall a. f a -> g a\n\
            infixr 0 type Natural as ~>\n",
    );
    let main = (
        "Main.purs",
        "module Main where\n\
            import Lib (Tuple(..), type (:*:), type (~>))\n\
            value :: Int :*: Boolean :*: String\n\
            value = Tuple 1 (Tuple true \"nested\")\n\
            natty :: forall f. f ~> f\n\
            natty x = x\n\
            main = 0\n",
    );
    let ast = lower_source_to_ast(main.0, main.1).unwrap();
    let value = ast
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "value")
        .unwrap();
    let psrs_ast::TypeKind::OperatorChain {
        operands,
        operators,
    } = &value.annotation.as_ref().unwrap().kind
    else {
        panic!("P2 must retain the unresolved type operator chain");
    };
    assert_eq!(operands.len(), 3);
    assert!(operators.iter().all(|operator| operator.name.text == ":*:"));
    let resolved = resolve_program_sources(&[library, main]).unwrap();
    let signature = resolved[1]
        .declarations
        .iter()
        .find(|declaration| declaration.name == "value")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::OperatorChain {
        operands,
        operators,
    } = &signature.kind
    else {
        panic!("P3 must retain the resolved type operator chain");
    };
    assert_eq!(operands.len(), 3);
    assert!(operators.iter().all(|operator| {
        operator.precedence == 6
            && operator.associativity == psrs_hir::Associativity::Right
            && operator.head == psrs_hir::ResolvedTypeHead::Named(resolved[0].types[0].id)
    }));

    let natural_id = resolved[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Natural")
        .unwrap()
        .id;
    let natty = resolved[1]
        .declarations
        .iter()
        .find(|declaration| declaration.name == "natty")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Forall { body, .. } = &natty.kind else {
        panic!("expected the quantified type operator signature");
    };
    let psrs_hir::TypeKind::OperatorChain { operators, .. } = &body.kind else {
        panic!("`~>` must remain a type operator chain through P3");
    };
    assert_eq!(
        operators[0].head,
        psrs_hir::ResolvedTypeHead::Named(natural_id)
    );
    assert_eq!(operators[0].precedence, 0);
    assert_eq!(operators[0].associativity, psrs_hir::Associativity::Right);

    let normalized = psrs_desugar::desugar_module(resolved[1].clone()).unwrap();
    let signature = normalized
        .declarations
        .iter()
        .find(|declaration| declaration.name == "value")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Application(_, right) = &signature.kind else {
        panic!("P4 must lower type operators to type applications");
    };
    assert!(matches!(right.kind, psrs_hir::TypeKind::Application(..)));
    let normalized = psrs_desugar::desugar_module(resolved[1].clone()).unwrap();
    let natty = normalized
        .declarations
        .iter()
        .find(|declaration| declaration.name == "natty")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Forall { body, .. } = &natty.kind else {
        panic!("expected P4 to keep the quantified type");
    };
    assert!(matches!(body.kind, psrs_hir::TypeKind::Application(..)));
    typecheck_program_sources(&[library, main]).unwrap();
}

#[test]
fn constructor_operator_patterns_resolve_reassociate_and_run() {
    let source = r#"module Main where
data Chain a = Link a (Chain a) | Empty
infixr 6 Link as :>
values = 1 :> 2 :> Empty
main = case values of
  first :> _ -> first
  _ -> 0
"#;
    let ast = lower_source_to_ast("Main.purs", source).unwrap();
    let main = ast
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "main")
        .unwrap();
    let psrs_ast::ExprKind::Case { branches, .. } = &main.value.kind else {
        panic!("expected a case expression");
    };
    assert!(matches!(
        branches[0].pattern.kind,
        psrs_ast::PatternKind::OperatorChain { .. }
    ));

    let resolved = resolve_program_sources(&[("Main.purs", source)]).unwrap();
    let main = resolved[0]
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .unwrap();
    let psrs_hir::ExprKind::Case { branches, .. } = &main.value.kind else {
        panic!("expected a resolved case");
    };
    let psrs_hir::PatternKind::OperatorChain { operators, .. } = &branches[0].pattern.kind else {
        panic!("P3 must preserve the constructor operator chain");
    };
    assert_eq!(operators[0].precedence, 6);
    assert_eq!(operators[0].associativity, psrs_hir::Associativity::Right);

    let output = run_with_wasmtime(source).expect("required Wasmtime runtime");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn official_type_operator_fixture_resolves_and_lowers_chains_through_p4() {
    let library = (
        "A.purs",
        include_str!("../../../../../tests/upstream/passing/TypeOperators/A.purs"),
    );
    let console = (
        "Effect.Console.purs",
        "module Effect.Console where\nlog :: String -> Int\nlog _ = 0\n",
    );
    let main = (
        "Main.purs",
        include_str!("../../../../../tests/upstream/passing/TypeOperators.purs"),
    );
    let resolved = resolve_program_sources(&[library, console, main]).unwrap();
    let tuple_id = resolved[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Tuple")
        .unwrap()
        .id;
    let natural_id = resolved[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Natural")
        .unwrap()
        .id;
    let main = &resolved[2];
    let natty = main
        .declarations
        .iter()
        .find(|declaration| declaration.name == "natty")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Forall { body, .. } = &natty.kind else {
        panic!("expected the fixture's quantified type operator signature");
    };
    assert!(matches!(
        body.kind,
        psrs_hir::TypeKind::OperatorChain { .. }
    ));

    let normalized = psrs_desugar::desugar_module(main.clone()).unwrap();
    let natty = normalized
        .declarations
        .iter()
        .find(|declaration| declaration.name == "natty")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Forall { body, .. } = &natty.kind else {
        panic!("expected P4 to keep the quantified type");
    };
    assert_eq!(type_application_head(body), Some(natural_id));

    for declaration in &normalized.types {
        if declaration.name.starts_with("UseOperatorIn") && !declaration.parameters.is_empty() {
            let kind = declaration.parameters[0].kind.as_ref().unwrap();
            assert_eq!(type_application_head(kind), Some(tuple_id));
        }
    }
}

fn type_application_head(ty: &psrs_hir::Type) -> Option<psrs_hir::TypeId> {
    match &ty.kind {
        psrs_hir::TypeKind::Application(function, _) => type_application_head(function),
        psrs_hir::TypeKind::Named(id) => Some(*id),
        _ => None,
    }
}

#[test]
fn reassociates_custom_fixities_and_expands_both_operator_sections_at_runtime() {
    let source = "module Main where\n\
        infixr 4 subtract as <+>\n\
        infixl 6 multiply as %%\n\
        subtract x y = x - y\n\
        multiply x y = x * y\n\
        main = (10 <+> 3 <+> 2) + ((10 <+> _) 3) + ((_ <+> 3) 10) + (10 <+> 3 %% 2)\n";
    let output = run_with_wasmtime(source).expect("required Wasmtime runtime");
    assert_eq!(output.status.code(), Some(27));
}

#[test]
fn lower_precedence_operators_separate_fixity_conflict_groups() {
    // This mirrors the official passing/LetPattern.purs shape: `==` is
    // non-associative, but each use is in a separate operand of lower-precedence `&&`.
    let source = (
        "Main.purs",
        "module Main where\n\
            infix 3 equal as ==\n\
            infixl 5 left as <+>\n\
            infixr 5 right as <*>\n\
            infixr 2 combine as &&\n\
            equal x y = x\n\
            left x y = x\n\
            right x y = y\n\
            combine x y = x\n\
            comparisons = 1 == 2 && 3 == 4\n\
            opposing = 1 <+> 2 && 3 <*> 4\n\
            main = comparisons\n",
    );
    let resolved = resolve_program_sources(&[source]).expect("fixity targets should resolve");
    let normalized = psrs_desugar::desugar_module(resolved[0].clone())
        .expect("lower-precedence operators separate fixity groups");
    assert!(normalized.declarations.iter().all(|declaration| !matches!(
        declaration.value.kind,
        psrs_hir::ExprKind::OperatorChain { .. }
    )));
}

#[test]
fn qualified_fixity_targets_use_value_constructor_and_type_namespaces() {
    let library = (
        "Lib.purs",
        "module Lib where\n\
            target x y = x\n\
            data Chain = Link Chain Chain | Empty\n\
            data Pair a b = Pair a b\n",
    );
    let main = (
        "Main.purs",
        "module Main where\n\
            import Lib as L\n\
            infixl 4 L.target as <+>\n\
            infixr 6 L.Link as :>\n\
            infixr 6 type L.Pair as :*: \n\
            value = 1 <+> 2\n\
            chain = L.Empty :> L.Empty\n\
            type Product = Int :*: Boolean\n\
            main = value\n",
    );
    let modules = resolve_program_sources(&[library, main])
        .expect("qualified alias targets should resolve through the imported module");
    let value_target = modules[1]
        .fixities
        .iter()
        .find(|fixity| fixity.operator == "<+>")
        .unwrap();
    let constructor_target = modules[1]
        .fixities
        .iter()
        .find(|fixity| fixity.operator == ":>")
        .unwrap();
    let type_target = modules[1]
        .fixities
        .iter()
        .find(|fixity| fixity.operator == ":*:")
        .unwrap();
    assert!(matches!(
        value_target.target,
        psrs_hir::FixityTarget::Value(_)
    ));
    assert!(matches!(
        constructor_target.target,
        psrs_hir::FixityTarget::Value(_)
    ));
    assert!(matches!(
        type_target.target,
        psrs_hir::FixityTarget::Type(_)
    ));
}

#[test]
fn ambiguous_imported_fixity_targets_report_scope_conflicts() {
    let first = (
        "A.purs",
        "module A where\ntarget x y = x\ndata Pair a b = Pair a b\n",
    );
    let second = (
        "B.purs",
        "module B where\ntarget x y = y\ndata Pair a b = Pair b a\n",
    );
    let ambiguous_value = (
        "MainValue.purs",
        "module MainValue where\n\
            import A\n\
            import B\n\
            infixl 4 target as <+>\n\
            main = 1 <+> 2\n",
    );
    let ambiguous_type = (
        "MainType.purs",
        "module MainType where\n\
            import A\n\
            import B\n\
            infixr 6 type Pair as :*: \n\
            type Product = Int :*: Boolean\n\
            main = 0\n",
    );

    for sources in [
        &[first, second, ambiguous_value][..],
        &[first, second, ambiguous_type][..],
    ] {
        let errors = resolve_program_sources(sources).unwrap_err();
        assert!(
            errors.iter().any(|error| {
                matches!(error.diagnostic.code.as_ref(), Some(code) if *code == "ScopeConflict")
            }),
            "expected a scope conflict, got {errors:?}"
        );
    }
}

#[test]
fn rejects_ambiguous_fixity_groups_for_values_patterns_and_types() {
    let cases = [
        (
            "module Main where\ninfixl 5 subtract as <+>\ninfixr 5 subtract as <*>\nsubtract x y = x - y\nmain = 1 <+> 2 <*> 3\n",
            "operators of the same precedence have mixed associativity",
        ),
        (
            "module Main where\ndata Chain = Link Chain Chain | Empty\ninfix 5 Link as :>\nmain = case Empty of\n  Empty :> Empty :> Empty -> 0\n  _ -> 1\n",
            "a non-associative operator cannot be chained",
        ),
        (
            "module Main where\ndata Pair a b = Pair a b\ninfixl 5 type Pair as :*: \ninfixr 5 type Pair as :+:\ntype Bad = Int :*: Boolean :+: String\nmain = 0\n",
            "operators of the same precedence have mixed associativity",
        ),
    ];
    for (source, expected_message) in cases {
        let errors = typecheck_program_sources(&[("Main.purs", source)]).unwrap_err();
        assert!(
            errors.iter().any(|error| {
                error.diagnostic.stage == "P4 desugar"
                    && error.diagnostic.message.contains(expected_message)
            }),
            "expected P4 diagnostic {expected_message:?}, got {errors:?}"
        );
    }
}
