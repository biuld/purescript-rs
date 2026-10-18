//! The determined-instance-head rule, the superclass-cycle class declaration
//! check, and the recursive-instance depth bound, each under its official
//! `errorCode`. These are the structural checks the corpus cases
//! `RowInInstanceNotDetermined*`, `TypeSynonyms7`, `3510`,
//! `InvalidDerivedInstance2`, `438`, `Superclasses2`, and
//! `SelfCycleInTypeClassDeclaration` exercise.

use super::*;

/// The program's errors carry exactly the one official code named.
fn assert_one_code(name: &str, expected: &str, sources: &[(&str, &str)]) {
    let errors = crate::check_program(sources).expect_err("the program should be rejected");
    let codes: Vec<Option<&str>> = errors.iter().map(|error| error.diagnostic.code).collect();
    assert!(
        codes == [Some(expected)],
        "`{name}`: expected exactly [{expected}], got {codes:?}"
    );
}

/// An argument should be rejected when its shape rides along through type
/// synonym expansion, or when no fundep determines it.
#[test]
fn nondetermined_instance_head_arguments_report_official_invalid_instance_head() {
    let cases: [(&str, &str); 6] = [
        (
            // An empty record argument a class without fundeps cannot
            // determine.
            "argument in no fundep",
            r#"module Main where
data MyUnit = MyUnit
class C a b
instance c :: C MyUnit {}
"#,
        ),
        (
            // The last argument is mentioned by no dependency.
            "argument in no fundep",
            r#"module Main where
data MyUnit = MyUnit
class C a b c | a -> b
instance c :: C MyUnit MyUnit {}
"#,
        ),
        (
            // `b` is determined by `a`, which it itself determines.
            "mutually determined argument",
            r#"module Main where
data MyUnit = MyUnit
class C a b | a -> b, b -> a
instance c :: C MyUnit {}
"#,
        ),
        (
            // A free unification variable survives the synonym into the
            // head's row.
            "synonym leaves a free variable in the row",
            r#"module Main where
type X r = {x :: Int | r}
class C a
instance c :: C (X r)
"#,
        ),
        (
            // A record the derivation cannot determine, spelled through a
            // synonym.
            "derived head over a synonym record",
            r#"module Main where
type T = {}
class Some a
derive instance someT :: Some T
"#,
        ),
        (
            "derived head over a record",
            r#"module Main where
class Some a
derive instance someRecord :: Some {}
"#,
        ),
    ];
    for (name, source) in cases {
        assert_one_code(name, "InvalidInstanceHead", &[("Main.purs", source)]);
    }
}

#[test]
fn determined_and_constructor_instance_heads_stay_accepted() {
    // Constructor arguments remain legal in every position.
    check_program(&[(
        "Main.purs",
        r#"module Main where
data MyUnit = MyUnit
class C a b
instance c :: C MyUnit MyUnit
"#,
    )])
    .unwrap_or_else(|errors| panic!("constructor heads should type check: {errors:?}"));
    // A row argument is legal where a fundep fully determines its position.
    check_program(&[(
        "Main.purs",
        r#"module Main where
data MyUnit = MyUnit
class C a b | a -> b
instance c :: C MyUnit {}
"#,
    )])
    .unwrap_or_else(|errors| panic!("a determined row argument should type check: {errors:?}"));
    // An applied constructor head and a record head over a bound variable.
    check_program(&[(
        "Main.purs",
        r#"module Main where
data Box a = Box a
class D a
instance dBox :: D (Box a)
instance dRecord :: D (Record r)
"#,
    )])
    .unwrap_or_else(|errors| panic!("record-over-variable heads should type check: {errors:?}"));
    // A helper class whose closed record argument sits in a determined
    // position, the shape the official record helpers use.
    check_program(&[(
        "Main.purs",
        r#"module Main where
data EndList = EndList
class Helper rl row sub | rl -> sub
instance helperNil :: Helper EndList row {}
"#,
    )])
    .unwrap_or_else(|errors| panic!("a determined closed record should type check: {errors:?}"));
}

#[test]
fn superclass_cycles_report_official_cycle_in_type_class_declaration() {
    assert_one_code(
        "a class may not have itself as a superclass",
        "CycleInTypeClassDeclaration",
        &[(
            "Main.purs",
            r#"module Main where
class (Foo a) <= Foo a
"#,
        )],
    );
    assert_one_code(
        "mutual superclasses",
        "CycleInTypeClassDeclaration",
        &[(
            "Main.purs",
            r#"module Main where
class (Bar a) <= Foo a
class (Foo a) <= Bar a
"#,
        )],
    );
}

#[test]
fn a_recursive_instance_context_reports_possibly_infinite_instance() {
    assert_one_code(
        "recursive instance",
        "PossiblyInfiniteInstance",
        &[(
            "Main.purs",
            r#"module Main where
data YesNo = Yes
data Fix f = In (f (Fix f))
data Wrapped a = Wrapped
class Eqish a where
  eqish :: a -> a -> YesNo
instance eqWrapped :: (Eqish a) => Eqish (Wrapped a) where
  eqish Wrapped Wrapped = Yes
instance eqFix :: (Eqish (f (Fix f))) => Eqish (Fix f) where
  eqish (In f) (In g) = eqish f g
main = eqish (In Wrapped) (In Wrapped)
"#,
        )],
    );
}
