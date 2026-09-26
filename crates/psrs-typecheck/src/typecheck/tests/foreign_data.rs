use super::super::{TypeCheckErrorKind, typecheck_module};
use psrs_hir::{ModuleId, TypeDeclarationKind, TypeKind};
use psrs_resolve::ResolveErrorKind;
use psrs_span::SourceFile;
use psrs_thir as thir;

fn lower_source(name: &str, source: &str) -> psrs_ast::Module {
    let file = SourceFile::new(name, source);
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    psrs_ast::lower_module(cst).expect("surface lowering")
}

fn resolve_one(source: &str) -> Result<psrs_hir::Module, Vec<psrs_resolve::ResolveError>> {
    let module = lower_source("Main.purs", source);
    psrs_resolve::resolve_module_with_externals(
        module,
        ModuleId(0),
        &psrs_resolve::bootstrap_externals(),
    )
}

fn check_types(module: psrs_hir::Module) -> psrs_thir::Module {
    let kinds = psrs_kind::check_module(&module);
    assert!(kinds.is_empty(), "unexpected kind errors: {kinds:?}");
    let module = psrs_desugar::desugar_module(module).expect("desugar");
    typecheck_module(module).expect("typecheck")
}

#[test]
fn declares_an_opaque_foreign_type_and_rejects_source_construction() {
    let resolved = resolve_one(
        "module Main where\n\
         foreign import \"wasi:cli/stdout#get-stdout\" getStdout :: Handle\n\
         foreign import data Handle :: Type\n\
         keep :: Handle -> Handle\n\
         keep h = h\n",
    )
    .expect("a foreign data declaration should resolve");
    let foreign = resolved
        .types
        .iter()
        .find(|declaration| declaration.name == "Handle")
        .expect("Handle should be a type declaration");
    assert_eq!(foreign.kind, TypeDeclarationKind::Foreign);
    assert!(foreign.constructors.is_empty());
    let foreign_id = foreign.id;
    {
        let signature = resolved
            .externals
            .iter()
            .find(|external| external.name == "getStdout")
            .and_then(|external| external.signature.as_ref())
            .expect("getStdout should keep its source signature");
        assert!(matches!(signature.kind, TypeKind::Opaque(id) if id == foreign_id));
    }
    let typed = check_types(resolved);
    assert!(
        typed.constructors.is_empty(),
        "an opaque type has no value constructors"
    );
    assert_eq!(typed.opaque_ids, vec![foreign_id]);
    let keep = typed
        .declarations
        .iter()
        .find(|declaration| declaration.name == "keep")
        .expect("keep should be elaborated");
    let thir::Type::Function { parameter, result } = &typed.types[keep.ty.0 as usize] else {
        panic!(
            "keep should be a function, got {:?}",
            typed.types[keep.ty.0 as usize]
        );
    };
    for end in [*parameter, *result] {
        assert!(
            matches!(
                &typed.types[end.0 as usize],
                thir::Type::Constructor(thir::TypeConstructor::User(id)) if *id == foreign_id
            ),
            "Handle must stay a nominal constructor, not {:?}",
            typed.types[end.0 as usize]
        );
    }

    let forged = resolve_one(
        "module Main where\n\
         foreign import data Handle :: Type\n\
         bad :: Handle\n\
         bad = 1\n",
    )
    .expect("the illegal value should still resolve");
    let kinds = psrs_kind::check_module(&forged);
    assert!(kinds.is_empty(), "{kinds:?}");
    let forged = psrs_desugar::desugar_module(forged).expect("desugar");
    let errors = typecheck_module(forged).expect_err("Int must not inhabit Handle");
    assert!(
        errors.iter().any(|error| {
            error.kind == TypeCheckErrorKind::TypeMismatch
                && error.message().contains("Handle")
                && error.message().contains("Int")
        }),
        "{errors:?}"
    );

    let effect = resolve_one(
        "module Main where\n\
         foreign import data Effect :: Type -> Type\n\
         pure :: forall a. a -> Effect a\n\
         pure x = x\n",
    )
    .expect("the higher-kinded declaration should resolve");
    let kinds = psrs_kind::check_module(&effect);
    assert!(kinds.is_empty(), "{kinds:?}");
    let effect = psrs_desugar::desugar_module(effect).expect("desugar");
    let errors = typecheck_module(effect).expect_err("a value must not build Effect a");
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "{errors:?}"
    );

    let named = resolve_one(
        "module Main where\n\
         foreign import data Handle :: Type\n\
         bad = Handle\n",
    )
    .expect_err("the type name is not a constructor");
    assert!(
        named
            .iter()
            .any(|error| error.kind == ResolveErrorKind::UnknownName),
        "{named:?}"
    );
}

#[test]
fn keeps_an_imported_foreign_type_opaque() {
    let streams = lower_source(
        "Streams.purs",
        "module Streams where\n\
         foreign import data OutputStream :: Type\n",
    );
    let main = lower_source(
        "Main.purs",
        "module Main where\n\
         import Streams\n\
         foreign import \"wasi:cli/stdout#get-stdout\" getStdout :: OutputStream\n\
         keep :: OutputStream -> OutputStream\n\
         keep s = s\n",
    );
    let resolved = psrs_resolve::resolve_program(vec![streams, main]).expect("resolve program");
    let foreign = resolved[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "OutputStream")
        .expect("the declaring module should record the opaque type");
    assert_eq!(foreign.kind, TypeDeclarationKind::Foreign);
    let signature = resolved[1]
        .externals
        .iter()
        .find(|external| external.name == "getStdout")
        .and_then(|external| external.signature.as_ref())
        .expect("the importer should see the foreign signature");
    assert!(matches!(signature.kind, TypeKind::Opaque(id) if id == foreign.id));
    assert!(
        resolved[1]
            .imports
            .iter()
            .flat_map(|import| import.types.iter())
            .any(|imported| imported.opaque && imported.id == foreign.id)
    );
    check_types(resolved[1].clone());
}

#[test]
fn rejects_a_wit_binding_on_foreign_data() {
    let file = SourceFile::new(
        "Main.purs",
        "module Main where\n\
         foreign import \"wasi:cli/stdout#get-stdout\" data Handle :: Type\n",
    );
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "{errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    let errors = psrs_ast::lower_module(cst).expect_err("data does not take a binding");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("does not take a WIT binding")),
        "{errors:?}"
    );
}
