use super::*;

#[test]
fn local_foreign_type_operator_preserves_opaque_hir_head() {
    let source = (
        "Main.purs",
        "module Main where\n\
            foreign import data Pair :: Type -> Type -> Type\n\
            infixr 6 type Pair as :*: \n\
            direct :: Pair Int String\n\
            direct = 0\n\
            operated :: Int :*: String\n\
            operated = 0\n\
            main = 0\n",
    );
    let modules = resolve_program_sources(&[source]).unwrap();
    let pair_id = modules[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Pair")
        .unwrap()
        .id;
    assert_pair_heads_match(&modules[0], pair_id);
}

#[test]
fn imported_reexported_foreign_type_operator_preserves_opaque_hir_head() {
    let library = (
        "Lib.purs",
        "module Lib (Pair, type (:*:)) where\n\
            foreign import data Pair :: Type -> Type -> Type\n\
            infixr 6 type Pair as :*: \n",
    );
    let facade = (
        "Facade.purs",
        "module Facade (module Lib) where\nimport Lib\n",
    );
    let main = (
        "Main.purs",
        "module Main where\n\
            import Facade (Pair, type (:*:))\n\
            direct :: Pair Int String\n\
            direct = 0\n\
            operated :: Int :*: String\n\
            operated = 0\n\
            main = 0\n",
    );
    let modules = resolve_program_sources(&[library, facade, main]).unwrap();
    let pair_id = modules[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Pair")
        .unwrap()
        .id;
    let imported_pair = modules[2].imports[0]
        .types
        .iter()
        .find(|imported| imported.name == "Pair")
        .unwrap();
    assert_eq!(
        imported_pair.reference,
        psrs_hir::TypeReference::Named(pair_id)
    );
    assert!(imported_pair.opaque);
    assert_pair_heads_match(&modules[2], pair_id);
}

fn assert_pair_heads_match(module: &psrs_hir::Module, pair_id: psrs_hir::TypeId) {
    let direct = module
        .declarations
        .iter()
        .find(|declaration| declaration.name == "direct")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let operated = module
        .declarations
        .iter()
        .find(|declaration| declaration.name == "operated")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let expected = psrs_hir::ResolvedTypeHead::Opaque(pair_id);
    assert_eq!(type_application_head(direct), Some(expected));

    let psrs_hir::TypeKind::OperatorChain { operators, .. } = &operated.kind else {
        panic!("P3 must retain the type operator chain");
    };
    assert_eq!(operators[0].head, expected);

    let normalized = psrs_desugar::desugar_module(module.clone()).unwrap();
    let operated = normalized
        .declarations
        .iter()
        .find(|declaration| declaration.name == "operated")
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    assert_eq!(type_application_head(operated), Some(expected));
}

fn type_application_head(ty: &psrs_hir::Type) -> Option<psrs_hir::ResolvedTypeHead> {
    match &ty.kind {
        psrs_hir::TypeKind::Application(function, _) => type_application_head(function),
        psrs_hir::TypeKind::Constructor(builtin) => {
            Some(psrs_hir::ResolvedTypeHead::Builtin(*builtin))
        }
        psrs_hir::TypeKind::Named(id) => Some(psrs_hir::ResolvedTypeHead::Named(*id)),
        psrs_hir::TypeKind::Opaque(id) => Some(psrs_hir::ResolvedTypeHead::Opaque(*id)),
        _ => None,
    }
}
