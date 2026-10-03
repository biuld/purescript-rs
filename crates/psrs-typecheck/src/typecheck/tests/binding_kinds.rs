//! Kind compatibility at an ordinary binding.
//!
//! Every type unknown carries a kind and a binding maintains it, so whether an
//! operation is kind-corrected must not depend on which module reached it. The
//! cases here check the binding rule directly — the kind recorded for a
//! variable against the kind of the type it is bound to, through the one kind
//! solver — and the end-to-end programs that show a checked kind surviving from
//! a declaration into a binding.

use super::super::{
    Checker, InferType, TypeCheckError, TypeCheckErrorKind, TypeConstructor, TypecheckContext,
    typecheck_module_with_checked_kinds_and_module_names,
};
use psrs_hir::{ModuleId, TypeId};
use psrs_kind::Kind;
use psrs_span::SourceFile;
use std::collections::HashMap;

fn resolve_one(source: &str) -> psrs_hir::Module {
    let file = SourceFile::new("Main.purs", source);
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    let ast = psrs_ast::lower_module(cst).expect("surface lowering");
    psrs_resolve::resolve_module_with_externals(
        ast,
        ModuleId(0),
        &psrs_resolve::bootstrap_externals(),
    )
    .expect("the program should resolve")
}

/// Kind checks the program, then type checks `Main` against the scheme the kind
/// pass produced.
fn typecheck_program(source: &str) -> Result<psrs_thir::Module, Vec<TypeCheckError>> {
    let program = [resolve_one(source)];
    let (checked_kinds, kind_diagnostics) = psrs_kind::check_program(&program);
    assert!(
        kind_diagnostics.is_empty(),
        "unexpected kind errors: {kind_diagnostics:?}"
    );
    let module_names = HashMap::from([(ModuleId(0), "Main".to_owned())]);
    let known_types = program[0].types.clone();
    let module = psrs_desugar::desugar_module(program[0].clone()).expect("desugar");
    typecheck_module_with_checked_kinds_and_module_names(
        module,
        &HashMap::new(),
        None,
        false,
        TypecheckContext {
            known_types: &known_types,
            imported_instances: &[],
            module_names: &module_names,
            checked_kinds: &checked_kinds,
        },
    )
}

/// A checker over an empty module, for exercising the binding rule directly.
fn checker() -> Checker {
    let program = [resolve_one("module Main where\n")];
    Checker::new(
        &program[0],
        &HashMap::new(),
        TypecheckContext {
            known_types: &[],
            imported_instances: &[],
            module_names: &HashMap::from([(ModuleId(0), "Main".to_owned())]),
            checked_kinds: &psrs_kind::CheckedKindEnv::default(),
        },
    )
}

fn row_kind() -> Kind {
    Kind::row(psrs_kind::type_kind())
}

fn int() -> super::super::InferType {
    InferType::Constructor(TypeConstructor::Int)
}

/// A row tail keeps the kind its row admits. Binding a variable whose recorded
/// kind is `Row Type` to a plain `Int` is a kind equation with no solution, so
/// the binding is refused and `KindsDoNotUnify` is reported.
#[test]
fn refuses_a_row_valued_binding_at_the_kind_of_a_plain_type() {
    let mut checker = checker();
    let InferType::Variable(variable) = checker.fresh() else {
        unreachable!("a fresh unknown is a variable")
    };
    checker.record_variable_kind(variable, row_kind());

    let span = psrs_span::TextRange::new(3, 6);
    assert!(
        !checker.bind_variable(variable, int(), span),
        "a row may not be bound to a plain type"
    );
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(
        checker.state.errors[0].kind,
        TypeCheckErrorKind::KindsDoNotUnify
    );
    assert_eq!(checker.state.errors[0].span, span);
    assert_eq!(
        checker.state.errors[0].error_code(),
        Some("KindsDoNotUnify")
    );
}

/// The same variable bound to the empty row, which does have kind `Row Type`.
#[test]
fn accepts_a_row_valued_binding_at_the_kind_its_row_admits() {
    let mut checker = checker();
    let InferType::Variable(variable) = checker.fresh() else {
        unreachable!("a fresh unknown is a variable")
    };
    checker.record_variable_kind(variable, row_kind());

    let span = psrs_span::TextRange::new(3, 6);
    assert!(checker.bind_variable(variable, InferType::RowEmpty, span));
    assert!(
        checker.state.errors.is_empty(),
        "{:?}",
        checker.state.errors
    );
}

/// A binding that leaves the kind unconstrained is recorded, and its kind
/// equation is left for a later binding to solve. This is the ordinary case:
/// most variables are created without a known kind and only acquire one here.
#[test]
fn records_a_binding_whose_kind_stays_a_fresh_unknown() {
    let mut checker = checker();
    let InferType::Variable(variable) = checker.fresh() else {
        unreachable!("a fresh unknown is a variable")
    };

    let span = psrs_span::TextRange::new(3, 6);
    assert!(checker.bind_variable(variable, int(), span));
    assert!(
        checker.state.errors.is_empty(),
        "{:?}",
        checker.state.errors
    );
    assert!(
        checker.state.variable_kinds.contains_key(&variable),
        "every type unknown keeps its kind"
    );
}

/// A type whose kind the checked environment does not supply is left alone: the
/// missing scheme is the kind pass's diagnostic, and inference must not reject
/// the same module a second time for it.
#[test]
fn leaves_a_binding_alone_when_the_environment_supplied_no_kind() {
    let mut checker = checker();
    let InferType::Variable(variable) = checker.fresh() else {
        unreachable!("a fresh unknown is a variable")
    };
    let unchecked = InferType::Constructor(TypeConstructor::User(TypeId::new(ModuleId(0), 7)));

    let span = psrs_span::TextRange::new(3, 6);
    assert!(checker.bind_variable(variable, unchecked, span));
    assert!(
        checker.state.errors.is_empty(),
        "{:?}",
        checker.state.errors
    );
}

/// The kind recorded for a user constructor is the one its declaring module
/// checked, so its argument is checked at that kind.
#[test]
fn rejects_a_user_constructor_used_at_a_kind_its_checked_scheme_forbids() {
    let errors = typecheck_program(
        "module Main where\n\
         data Wrap = Wrap (Int -> Int)\n\
         main :: Wrap\n\
         main = Wrap 1\n",
    )
    .expect_err("Int is not an Int -> Int");
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "{errors:?}"
    );
}

/// The same constructor at its own kind is accepted, so the added check rejects a
/// genuinely ill-kinded binding and nothing else.
#[test]
fn accepts_a_user_constructor_used_at_its_checked_kind() {
    typecheck_program(
        "module Main where\n\
         data Wrap = Wrap (Int -> Int)\n\
         main :: Wrap\n\
         main = Wrap (\\x -> x)\n",
    )
    .expect("a constructor used at its own kind");
}

/// An unannotated binder gets a fresh kind that the binding determines, so the
/// ordinary `forall a. a -> a` at `Int` is not a kind error.
#[test]
fn accepts_an_unannotated_binder_used_at_its_solved_kind() {
    typecheck_program(
        "module Main where\n\
         f :: forall a. a -> a\n\
         f x = x\n\
         main :: Int\n\
         main = f 1\n",
    )
    .expect("an unannotated binder takes the kind it is used at");
}

/// A declared kind annotation is the kind pass's own check, and it fires before
/// inference: `a` is claimed to be a row and is then used as an arrow's
/// parameter. The type checker never sees this module.
#[test]
fn a_declared_kind_annotation_is_rejected_by_the_kind_pass() {
    let program = [resolve_one(
        "module Main where\n\
         f :: forall (a :: Row Type). a -> a\n\
         f x = x\n\
         main = f 1\n",
    )];
    let (_, kind_diagnostics) = psrs_kind::check_program(&program);
    assert!(
        kind_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "KindsDoNotUnify"),
        "{kind_diagnostics:?}"
    );
}
