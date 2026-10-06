#[test]
fn library_foreign_values_preserve_identity_and_checked_signatures() {
    let sources = [
        (
            "Native.purs",
            "module Native where\nforeign import step :: Int -> Int\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Native\nmain = step 42\n",
        ),
    ];
    let modules = crate::typecheck_program_sources(&sources).expect("declared foreign type checks");
    let native = &modules[0];
    let external = native
        .externals
        .iter()
        .find(|external| external.name == "step")
        .unwrap();
    assert_eq!(
        external.kind,
        psrs_hir::ExternalKind::Library {
            module: "Native".into()
        }
    );
    assert!(
        native
            .external_types
            .iter()
            .any(|checked| checked.symbol == external.symbol)
    );
    let mut core = crate::prepare_sources(&sources)
        .expect("foreign signatures lower to Core")
        .core;
    core.external_types
        .retain(|checked| checked.symbol != external.symbol);
    assert!(
        core.verify().is_err(),
        "a library declaration needs its checked signature"
    );
}

#[test]
fn a_library_foreign_value_can_own_an_operator_fixity() {
    let sources = [(
        "Main.purs",
        "module Main where\nforeign import operation :: Int -> Int -> Int\ninfixl 4 operation as %%\nmain = 1 %% 2\n",
    )];
    crate::check_program(&sources).expect("foreign values are declared before fixity resolution");
}

#[test]
fn a_source_foreign_declaration_shadows_a_bootstrap_primitive() {
    let sources = [(
        "Main.purs",
        "module Main where\nforeign import intAdd :: Int -> Int\nmain :: Int\nmain = intAdd 42\n",
    )];
    crate::check_program(&sources)
        .expect("the source declaration owns its declared arity and type");
}

#[test]
fn a_reexported_foreign_value_shadows_a_bootstrap_primitive() {
    let sources = [
        (
            "Native.purs",
            "module Native where\nforeign import intAdd :: Int -> Int\n",
        ),
        (
            "Facade.purs",
            "module Facade (module Native) where\nimport Native\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Facade\nmain :: Int\nmain = intAdd 42\n",
        ),
    ];
    crate::check_program(&sources).expect("imports keep their declaring symbol and signature");
}

#[test]
fn a_library_foreign_signature_rejects_source_class_constraints() {
    let sources = [(
        "Main.purs",
        "module Main where\nclass Equal a where\n  equal :: a -> a -> Boolean\nforeign import compareValues :: forall a. Equal a => a -> a -> Boolean\nmain = 0\n",
    )];
    let errors = crate::check_program(&sources).expect_err("purs rejects foreign constraints");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("class constraints"))
    );
}

#[test]
fn an_unimplemented_library_foreign_value_is_an_explicit_linking_error() {
    let sources = [(
        "Main.purs",
        "module Main where\nforeign import step :: Int -> Int\nmain = step 42\n",
    )];
    let errors =
        crate::compile_program_sources(&sources).expect_err("no target implementation exists");
    assert!(
        errors.iter().any(|error| {
            error.diagnostic.stage == "P8 library linking"
                && error.diagnostic.message.contains("Main.step")
                && error
                    .diagnostic
                    .message
                    .contains("no target implementation")
        }),
        "{errors:?}"
    );
}
