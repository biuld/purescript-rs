//! The official orphan instance rule, from `purs`'s
//! `TypeChecker.typeCheckAll` (`findNonOrphanModules` and
//! `checkOrphanInstance`): an instance declared in a module that defines
//! neither its class nor a type constructor covering one of the class's
//! functional-dependency covering sets is an orphan, and the official
//! `OrphanInstance` code is raised at the common instance recorder before the
//! instance is recorded, so it can neither overlap with another visible
//! instance nor submit a derived dictionary.

#[test]
fn rejects_an_instance_whose_head_names_no_declared_type() {
    // The class is foreign and the head argument is a primitive the declaring
    // module does not own, so no covering set is covered by the declaring
    // module.
    let errors = crate::check_program(&[
        ("Library.purs", "module Library where\nclass C a where\n  op :: a -> a\n"),
        (
            "Main.purs",
            "module Main where\nimport Library\ninstance cInt :: C Int where\n  op a = a\nmain = 0\n",
        ),
    ])
    .expect_err("a foreign-class instance over a primitive argument is an orphan");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanInstance")),
        "{errors:?}"
    );
}

#[test]
fn accepts_an_instance_of_a_class_declared_in_the_same_module() {
    // A module that declares its own class is never an orphan, whatever the
    // head argument names.
    crate::check_program(&[(
        "Main.purs",
        "module Main where\nclass C a where\n  op :: a -> a\ninstance cInt :: C Int where\n  op a = a\nmain = op 1\n",
    )])
    .expect("a locally-declared class's instance is not an orphan");
}

#[test]
fn accepts_an_instance_covered_by_a_local_head_constructor() {
    // The class is foreign, but the instance head's own type constructor is
    // declared in the declaring module, which satisfies the class's only
    // covering set.
    crate::check_program(&[
        ("Library.purs", "module Library where\nclass C a where\n  op :: a -> a\n"),
        (
            "Main.purs",
            "module Main where\nimport Library\ndata Local = Local\ninstance cLocal :: C Local where\n  op a = a\nmain = 0\n",
        ),
    ])
    .expect("a locally-declared head constructor makes the instance non-orphan");
}

#[test]
fn rejects_a_nullary_foreign_class_instance() {
    // A class with no parameters contributes no covering positions, so a
    // foreign nullary instance is always an orphan.
    let errors = crate::check_program(&[
        (
            "Library.purs",
            "module Library where\nclass C where\n  op :: Int\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Library\ninstance c :: C where\n  op = 1\nmain = 0\n",
        ),
    ])
    .expect_err("a foreign nullary class instance is an orphan");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanInstance")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_head_argument_that_names_no_module() {
    // A head argument that is a decided literal names no declaring module, so
    // it covers nothing — the official type-level `Symbol` shape.
    let errors = crate::check_program(&[
        (
            "Library.purs",
            "module Library where\nclass Foo (s :: Symbol) where\n  foo :: String\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Library\ninstance fooX :: Foo \"x\" where\n  foo = \"\"\nmain = 0\n",
        ),
    ])
    .expect_err("a literal head argument covers no covering set");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanInstance")),
        "{errors:?}"
    );
}

#[test]
fn rejects_an_instance_outside_the_covering_set_intersection() {
    // The official fundep shape: the class's dependencies give it two covering
    // sets, and the declaring module covers one but not the other, so it is
    // in no intersection — the intersection of covering-set modules is empty.
    let errors = crate::check_program(&[
        (
            "Library.purs",
            "module Library where\nclass C l r | l -> r, r -> l\ndata R = R\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Library\ndata L = L\ninstance clr :: C L R\nmain = 0\n",
        ),
    ])
    .expect_err("a module that covers only one covering set declares an orphan");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanInstance")),
        "{errors:?}"
    );
}

#[test]
fn accepts_an_instance_covering_every_set_through_a_local_constructor() {
    // The complement of the intersection rule: a locally-declared head
    // constructor in every covering set keeps the instance non-orphan, even
    // though the remaining arguments are foreign.
    crate::check_program(&[
        (
            "Library.purs",
            "module Library where\nclass C a b\ndata Foreign = Foreign\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Library\ndata Local = Local\ninstance cab :: C Local Foreign\nmain = 0\n",
        ),
    ])
    .expect("a locally-declared constructor in the only covering set is not an orphan");
}

#[test]
fn an_orphan_derived_instance_reports_only_the_orphan_code() {
    // A `derive instance` whose head, after synonym expansion, names a
    // primitive type is an orphan before any derivation runs, mirroring the
    // official throw order: the deriving half is never started, so the
    // report set is exactly `OrphanInstance`.
    let errors = crate::check_program(&[
        ("Data.Eq.purs", "module Data.Eq where\nclass Eq a where\n  eq :: a -> a -> Boolean\n"),
        (
            "Main.purs",
            "module Main where\nimport Data.Eq\ntype Something = Int\nderive instance eqSomething :: Eq Something\nmain = 0\n",
        ),
    ])
    .expect_err("deriving over a foreign primitive type is an orphan");
    assert!(
        errors
            .iter()
            .all(|error| error.diagnostic.code == Some("OrphanInstance")),
        "an orphan must be rejected before the deriving half runs: {errors:?}"
    );
}
