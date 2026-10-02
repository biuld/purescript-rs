use super::*;

#[test]
fn builtin_type_operator_aliases_keep_prim_identity_through_reexport() {
    let aliases = (
        "Aliases.purs",
        "module Aliases (type (~>), type (:+)) where\n\
            infixr 0 type Prim.Function as ~>\n\
            infixl 6 type Prim.Int as :+\n",
    );
    let facade = (
        "Facade.purs",
        "module Facade (module Aliases) where\nimport Aliases\n",
    );
    let main = (
        "Main.purs",
        "module Main where\n\
            import Facade (type (~>), type (:+))\n\
            identity :: forall f. f ~> f\n\
            identity value = value\n\
            integerOperator :: 1 :+ 2\n\
            integerOperator = 1\n\
            main = 0\n",
    );
    let resolved = resolve_program_sources(&[aliases, facade, main]).unwrap();
    let function = psrs_hir::TypeReference::Builtin(psrs_hir::BuiltinType::Function);
    let integer = psrs_hir::TypeReference::Builtin(psrs_hir::BuiltinType::Int);
    assert!(resolved[0].fixities.iter().any(|fixity| {
        fixity.operator == "~>" && fixity.target == psrs_hir::FixityTarget::Type(function)
    }));
    assert!(resolved[0].fixities.iter().any(|fixity| {
        fixity.operator == ":+" && fixity.target == psrs_hir::FixityTarget::Type(integer)
    }));
    assert!(
        resolved[1]
            .exports
            .as_ref()
            .unwrap()
            .type_operators
            .iter()
            .any(|op| { op.name == "~>" && op.reference == function })
    );
    assert!(
        resolved[1]
            .exports
            .as_ref()
            .unwrap()
            .type_operators
            .iter()
            .any(|op| { op.name == ":+" && op.reference == integer })
    );
    assert!(resolved[2].imports[0].fixities.iter().any(|fixity| {
        fixity.operator == "~>" && fixity.target == psrs_hir::FixityTarget::Type(function)
    }));
    assert!(resolved[2].imports[0].fixities.iter().any(|fixity| {
        fixity.operator == ":+" && fixity.target == psrs_hir::FixityTarget::Type(integer)
    }));

    let identity = resolved[2]
        .declarations
        .iter()
        .find(|declaration| declaration.name == "identity")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Forall { body, .. } = &identity.kind else {
        panic!("expected a polymorphic Function alias");
    };
    let psrs_hir::TypeKind::OperatorChain { operators, .. } = &body.kind else {
        panic!("P3 must retain the Function alias chain");
    };
    assert_eq!(operators[0].reference, function);

    let normalized = psrs_desugar::desugar_module(resolved[2].clone()).unwrap();
    let identity = normalized
        .declarations
        .iter()
        .find(|declaration| declaration.name == "identity")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let psrs_hir::TypeKind::Forall { body, .. } = &identity.kind else {
        panic!("expected P4 to preserve forall");
    };
    assert_eq!(
        builtin_type_application_head(body),
        Some(psrs_hir::BuiltinType::Function)
    );
    let integer_operator = normalized
        .declarations
        .iter()
        .find(|declaration| declaration.name == "integerOperator")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert_eq!(
        builtin_type_application_head(integer_operator),
        Some(psrs_hir::BuiltinType::Int)
    );

    let errors = check_program_kinds_lenient(&[aliases, facade, main])
        .err()
        .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("KindsDoNotUnify")),
        "Prim.Int is nullary, so its binary alias should retain identity through P4 and fail kind arity checking: {errors:?}"
    );
}

fn builtin_type_application_head(ty: &psrs_hir::Type) -> Option<psrs_hir::BuiltinType> {
    match &ty.kind {
        psrs_hir::TypeKind::Application(function, _) => builtin_type_application_head(function),
        psrs_hir::TypeKind::Constructor(builtin) => Some(*builtin),
        _ => None,
    }
}
