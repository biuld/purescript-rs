//! Residual-constraint generalization for a declaration that declares no
//! signature.
//!
//! `typesOf` in official PureScript solves the wanteds a given, a superclass
//! path, an instance, or a primitive relation discharges and retains the rest as
//! the declaration's scheme constraints, abstracting one dictionary parameter
//! for each. These cases pin the line between the two: what is retained, what
//! is still `NoInstance`, and what is refused before anything is generalized.
//!
//! Every case here is one `purs` 0.15.16 accepts or rejects with the same code.

use psrs_driver::check_source;

/// The source range a diagnostic points at, together with the `errorCode` the
/// driver's stage reports, so a case states which obligation failed and where.
fn rejection(source_name: &str, source: &str) -> Vec<(u32, &'static str)> {
    let errors = check_source(source_name, source).expect_err("the case must be rejected");
    errors
        .iter()
        .map(|error| (error.span.start, error.code.unwrap_or("<none>")))
        .collect()
}

fn accepts(source_name: &str, source: &str) {
    if let Err(errors) = check_source(source_name, source) {
        panic!("{source_name} was rejected: {errors:#?}");
    }
}

const CLASS_C: &str = "module Main where\n\nclass C a where\n  method :: a -> a\n\n";

/// The worked example from `type-inference.md`: `f x = method x` for
/// `class C a where method :: a -> a` infers `forall a. C a => a -> a`, so one
/// declaration serves uses at two different types.
#[test]
fn infers_a_qualified_type_for_a_signatureless_declaration() {
    accepts(
        "residual-generalizes.purs",
        &format!(
            "{CLASS_C}\
             instance cInt :: C Int where\n  method x = x\n\
             instance cString :: C String where\n  method x = x\n\
             f x = method x\n\
             atInt :: Int\natInt = f 1\n\
             atString :: String\natString = f \"hi\"\n"
        ),
    );
}

/// The retained constraint is the declaration's own: `f` takes a dictionary
/// parameter, selects `method` from it, and applies it. `purs` accepts the same
/// program and reports `forall a. C a => a -> a` for `f`.
#[test]
fn abstracts_one_dictionary_parameter_per_retained_constraint() {
    let compilation = psrs_driver::compile_source_with_dumps(
        "residual-dictionary.purs",
        &format!(
            "{CLASS_C}\
             instance cInt :: C Int where\n  method x = x\n\
             instance cBoolean :: C Boolean where\n  method x = x\n\
             f x = method x\n\
             main = if f true then f 41 else 0\n"
        ),
    )
    .expect("the program compiles");
    // The declaration's Core is one lambda per retained constraint, and the
    // evidence the body used is a field of that parameter rather than a global
    // instance dictionary.
    let core = compilation.dumps.get("core").expect("a core dump");
    let declaration = core
        .split_once("name: \"f\"")
        .expect("the declaration is in the core dump")
        .1;
    let declaration = &declaration[..declaration.len().min(4000)];
    assert!(
        declaration.contains("name: \"dict\""),
        "the retained constraint did not become a dictionary parameter:\n{declaration}"
    );
    assert!(
        declaration.contains("field: \"method\""),
        "the body did not select the method from the abstracted dictionary:\n{declaration}"
    );
}

/// A constraint whose arguments are all decided has nothing to generalize, so
/// it is a missing instance rather than a deferred one. `purs` rejects
/// `main = method 1` with `NoInstanceFound` for exactly this reason.
#[test]
fn reports_a_missing_instance_when_the_constraint_is_fully_decided() {
    let rejections = rejection(
        "residual-decided.purs",
        &format!("{CLASS_C}main = method 1\n"),
    );
    assert!(
        rejections
            .iter()
            .all(|(_, code)| *code == "NoInstanceFound"),
        "expected NoInstanceFound, got {rejections:?}"
    );
}

/// A declared signature states its own constraints, so an obligation its own
/// dictionary parameters do not discharge is a missing instance. `purs` rejects
/// this with `NoInstanceFound`, naming the rigid variable the constraint
/// mentions.
#[test]
fn a_declared_signature_still_must_prove_its_constraints() {
    let rejections = rejection(
        "residual-declared.purs",
        &format!("{CLASS_C}f :: forall a. a -> a\nf x = method x\n"),
    );
    assert!(
        rejections
            .iter()
            .all(|(_, code)| *code == "NoInstanceFound"),
        "expected NoInstanceFound, got {rejections:?}"
    );
}

/// Generalizing a variable nothing determines would quantify something no use
/// could instantiate. `purs` raises `AmbiguousTypeVariables` before it
/// generalizes, and the closure over the class functional dependencies is the
/// one that decides the set.
#[test]
fn reports_ambiguous_variables_before_generalizing() {
    let rejections = rejection(
        "residual-ambiguous.purs",
        &format!("{CLASS_C}f y = let g x = method x in y\n"),
    );
    assert!(
        rejections
            .iter()
            .all(|(_, code)| *code == "AmbiguousTypeVariables"),
        "expected AmbiguousTypeVariables, got {rejections:?}"
    );
}

/// A recursive binding group must prove its constraints: generalizing them
/// would admit polymorphic recursion over a constraint the recursive uses never
/// proved. `purs` raises `CannotGeneralizeRecursiveFunction` here.
#[test]
fn reports_a_recursive_group_that_cannot_prove_its_constraints() {
    let rejections = rejection(
        "residual-recursive.purs",
        &format!("{CLASS_C}foo x = method (bar x)\nbar x = method (foo x)\n"),
    );
    assert!(
        rejections
            .iter()
            .all(|(_, code)| *code == "CannotGeneralizeRecursiveFunction"),
        "expected CannotGeneralizeRecursiveFunction, got {rejections:?}"
    );
}

/// The same group with signatures is fine: a declared signature discharges its
/// own constraints, so nothing is left to generalize.
#[test]
fn a_recursive_group_with_signatures_keeps_its_constraints() {
    accepts(
        "residual-recursive-signed.purs",
        &format!(
            "{CLASS_C}\
             foo :: forall a. a -> a\nfoo x = bar x\n\
             bar :: forall a. a -> a\nbar x = foo x\n\
             main :: Int\nmain = foo 1\n"
        ),
    );
}

/// The residual constraint travels with the declaration, so exporting the value
/// requires the class to be exported too.
#[test]
fn an_exported_value_requires_its_retained_constraint_to_be_exported() {
    let rejections = rejection(
        "residual-export.purs",
        "module Main (f) where\n\nclass C a where\n  method :: a -> a\n\nf x = method x\n",
    );
    assert!(
        rejections
            .iter()
            .any(|(_, code)| *code == "TransitiveExportError"),
        "expected TransitiveExportError, got {rejections:?}"
    );
}
