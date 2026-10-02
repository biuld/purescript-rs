use super::*;

/// The two modules of `failing/DiffKindsSameName.purs`: each declares a type
/// called `DemoKind`, so the same name denotes two different kinds.
const DIFF_KINDS: [(&str, &str); 3] = [
    (
        "DiffKindsSameName.purs",
        "module DiffKindsSameName where\n\
         import DiffKindsSameName.LibA as LibA\n\
         import DiffKindsSameName.LibB as LibB\n\
         \n\
         data AProxy (m :: LibA.DemoKind) = AProxy\n\
         \n\
         bProxy :: AProxy LibB.DemoData\n\
         bProxy = AProxy\n",
    ),
    (
        "DiffKindsSameName/LibA.purs",
        "module DiffKindsSameName.LibA where\n\ndata DemoKind\n",
    ),
    (
        "DiffKindsSameName/LibB.purs",
        "module DiffKindsSameName.LibB where\n\ndata DemoKind\n\nforeign import data DemoData :: DemoKind\n",
    ),
];

#[test]
fn reports_a_cross_module_kind_conflict_against_the_module_that_declares_it() {
    // `AProxy LibB.DemoData` uses a kind at a place `LibA.DemoKind` is
    // declared. Only a program-level run can reject it: checked per module,
    // `LibB.DemoData` has no scheme in the checking module's environment and a
    // fresh variable would absorb the conflict.
    let modules = resolve_program(&DIFF_KINDS);
    let diagnostics = check_modules(&modules);
    assert!(
        diagnostics
            .iter()
            .any(|error| error.code == "KindsDoNotUnify"),
        "{diagnostics:?}"
    );
    // The declaration that is wrong is the parameter's, so the diagnostic names
    // the module whose source contains the offending annotation.
    let error = diagnostics
        .iter()
        .find(|error| error.code == "KindsDoNotUnify")
        .expect("a kind conflict");
    assert_eq!(error.origin, ModuleId(0), "{diagnostics:?}");
}

/// A program whose only kind-significant declaration is in another module, so
/// the only way `Main` can know `Lib.Box`'s kind is the environment it is given.
fn boxed_program() -> Vec<psrs_hir::Module> {
    resolve_program(&[
        (
            "Main.purs",
            "module Main where\n\
             import Lib\n\
             use :: Lib.Box Int -> Int\n\
             use b = 1\n",
        ),
        (
            "Lib.purs",
            "module Lib where\n\ndata Box :: Type -> Type\ndata Box a = Box a\n",
        ),
    ])
}

#[test]
fn a_per_module_check_reuses_the_kind_its_imported_declarations_were_checked_with() {
    let modules = boxed_program();
    let (environment, program_diagnostics) = check_env(&modules);
    assert!(
        program_diagnostics.is_empty(),
        "unexpected kind errors: {program_diagnostics:?}"
    );
    assert!(crate::check_module(&modules[0], &environment).is_empty());

    // The published kind is what decides the check, not a kind the checking
    // module derives for itself: publishing a different one rejects the use that
    // the program's own scheme accepts.
    let box_id = modules[1].types[0].id;
    let mut forged = environment.clone();
    forged.kinds.insert(
        box_id,
        crate::KindScheme::monomorphic(crate::Kind::Function(
            Box::new(crate::Kind::Builtin(psrs_hir::BuiltinType::Int)),
            Box::new(crate::type_kind()),
        )),
    );
    let diagnostics = crate::check_module(&modules[0], &forged);
    assert!(
        codes(&diagnostics).contains(&"KindsDoNotUnify"),
        "{diagnostics:?}"
    );
}

#[test]
fn reports_a_referenced_declaration_with_no_checked_scheme() {
    let modules = resolve_program(&[
        (
            "Main.purs",
            "module Main where\n\
             import Lib\n\
             keep :: Lib.Box Int -> Int\n\
             keep b = 1\n",
        ),
        ("Lib.purs", "module Lib where\n\ndata Box a = Box a\n"),
    ]);
    // Without the program's environment there is no kind for `Lib.Box`. That is
    // absent interface metadata, not an inferable parameter, so it is reported
    // instead of being given a fresh variable.
    let diagnostics = crate::check_module(&modules[0], &crate::CheckedKindEnv::default());
    assert!(
        diagnostics.iter().any(|error| error.code == "UnknownName"),
        "{diagnostics:?}"
    );
    // With the program's own environment the same use is checked against the
    // kind its declaring module published, and reports nothing.
    let (environment, _) = check_env(&modules);
    assert!(crate::check_module(&modules[0], &environment).is_empty());
}

#[test]
fn a_primitive_read_as_a_kind_denotes_itself() {
    use crate::{Kind, KindScope, KindState, denote_kind, primitive_kind, type_kind};
    use psrs_hir::{BuiltinType, Type, TypeKind};

    let node = |kind: TypeKind| Type {
        kind,
        span: psrs_span::TextRange::default(),
    };
    let mut state = KindState::default();
    let mut scope = KindScope::new(&mut state);
    // `Row k` is `App(Builtin(Row), k)`, and the row of types is what `Record`
    // consumes. There is no reserved constant and no dedicated `Row` head, so
    // the same kind expression denotes one thing everywhere.
    assert_eq!(
        denote_kind(
            &node(TypeKind::Application(
                Box::new(node(TypeKind::Constructor(BuiltinType::Row))),
                Box::new(node(TypeKind::Constructor(BuiltinType::Type))),
            )),
            &mut scope,
        ),
        Some(Kind::row(type_kind()))
    );
    // A primitive read as a kind is itself, a type-level string is a `Symbol`,
    // and a type-level integer is an `Int`.
    assert_eq!(
        denote_kind(
            &node(TypeKind::Constructor(BuiltinType::Symbol)),
            &mut scope
        ),
        Some(Kind::Builtin(BuiltinType::Symbol))
    );
    assert_eq!(
        denote_kind(&node(TypeKind::String("label".into())), &mut scope),
        Some(Kind::Builtin(BuiltinType::Symbol))
    );
    assert_eq!(
        denote_kind(&node(TypeKind::Integer("1".into())), &mut scope),
        Some(Kind::Builtin(BuiltinType::Int))
    );
    // `primitive_kind` is what a primitive has when it is used as a type, which
    // is official `primTypes` including `Unit`.
    assert_eq!(primitive_kind(BuiltinType::Unit), type_kind());
    assert_eq!(
        primitive_kind(BuiltinType::Record),
        Kind::Function(Box::new(Kind::row(type_kind())), Box::new(type_kind()))
    );
    assert_eq!(
        primitive_kind(BuiltinType::Row),
        Kind::Function(Box::new(type_kind()), Box::new(type_kind()))
    );
}
