//! Source-level evidence checks for the class slice (FE-14/15).
//!
//! These inspect the Typed Core produced from source to confirm that the
//! frontend selects the evidence forms the backend dictionary path expects:
//! givens, global instance dictionaries, instance constructors applied to
//! solved contexts, and superclass projections.

use std::collections::BTreeSet;

const EVIDENCE_SOURCE: &str = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

class Eq a <= Ord a where
  compare :: a -> a -> Int

class ToInt a where
  toInt :: a -> Int

instance eqInt :: Eq Int where
  eq x y = true

instance ordInt :: Ord Int where
  compare x y = 42

instance toIntFromEq :: Eq a => ToInt a where
  toInt x = 1

lessThan :: forall a. Ord a => a -> a -> Int
lessThan x y = if eq x y then compare x y else 0

main :: Int
main = toInt (lessThan 1 2)
"#;

#[derive(Default)]
struct Seen {
    given: bool,
    global: bool,
    superclass: bool,
    instance: bool,
    coercible: bool,
    primitive: bool,
}

fn collect(module: &psrs_thir::Module) -> Seen {
    let mut seen = Seen::default();
    for declaration in &module.declarations {
        walk_expr(&declaration.value, &mut seen);
    }
    seen
}

fn walk_expr(expression: &psrs_thir::Expr, seen: &mut Seen) {
    use psrs_thir::ExprKind;
    match &expression.kind {
        ExprKind::Evidence(evidence) => walk_evidence(evidence, seen),
        ExprKind::Coerce {
            value, evidence, ..
        } => {
            walk_expr(value, seen);
            walk_evidence(evidence, seen);
            seen.coercible = true;
        }
        ExprKind::UnsafeCoerce { value, .. } => {
            walk_expr(value, seen);
            seen.coercible = true;
        }
        ExprKind::Array(elements) => {
            for element in elements {
                walk_expr(element, seen);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                walk_expr(value, seen);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            walk_expr(expression, seen);
            for (_, value) in fields {
                walk_expr(value, seen);
            }
        }
        ExprKind::FieldAccess { expression, .. } => walk_expr(expression, seen),
        ExprKind::Application(function, argument) => {
            walk_expr(function, seen);
            walk_expr(argument, seen);
        }
        ExprKind::Lambda { body, .. } => walk_expr(body, seen),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                walk_expr(&binding.value, seen);
            }
            walk_expr(body, seen);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk_expr(condition, seen);
            walk_expr(then_branch, seen);
            walk_expr(else_branch, seen);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            walk_expr(scrutinee, seen);
            for branch in branches {
                walk_expr(&branch.value, seen);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn walk_evidence(evidence: &psrs_thir::Evidence, seen: &mut Seen) {
    use psrs_thir::EvidenceKind;
    match &evidence.kind {
        EvidenceKind::Given(_) => seen.given = true,
        EvidenceKind::Global(_) => seen.global = true,
        EvidenceKind::Superclass { parent, .. } => {
            seen.superclass = true;
            walk_evidence(parent, seen);
        }
        EvidenceKind::Instance { context, .. } => {
            seen.instance = true;
            for child in context {
                walk_evidence(child, seen);
            }
        }
        EvidenceKind::Coercible { .. } => seen.coercible = true,
        EvidenceKind::Primitive { .. } => seen.primitive = true,
    }
}

#[test]
fn selects_an_instance_declared_in_an_imported_module() {
    let library = "module A where\n\
        class ToInt a where\n\
        \x20 toInt :: a -> Int\n\
        instance toIntInt :: ToInt Int where\n\
        \x20 toInt x = x\n\
        convert :: forall a. ToInt a => a -> Int\n\
        convert x = toInt x\n";
    let main = "module Main where\n\
        import A\n\
        main :: Int\n\
        main = convert 42\n";
    let modules = crate::typecheck_program_sources(&[("A.purs", library), ("Main.purs", main)])
        .unwrap_or_else(|errors| {
            panic!("cross-module instance selection should type check: {errors:?}")
        });
    let seen = collect(&modules[1]);
    assert!(
        seen.global,
        "the imported instance dictionary must be selected as global evidence"
    );
}

#[test]
fn selects_an_imported_instance_with_a_context() {
    let library = "module A where\n\
        class Eq a where\n\
        \x20 eq :: a -> a -> Boolean\n\
        class ToInt a where\n\
        \x20 toInt :: a -> Int\n\
        instance eqInt :: Eq Int where\n\
        \x20 eq x y = true\n\
        instance toIntFromEq :: Eq a => ToInt a where\n\
        \x20 toInt x = 1\n";
    let main = "module Main where\n\
        import A\n\
        main :: Int\n\
        main = toInt 42\n";
    let modules = crate::typecheck_program_sources(&[("A.purs", library), ("Main.purs", main)])
        .unwrap_or_else(|errors| {
            panic!("an imported instance with a context should type check: {errors:?}")
        });
    let seen = collect(&modules[1]);
    assert!(
        seen.instance && seen.global,
        "the imported context instance and its remote context dictionary must be selected"
    );
}

#[test]
fn source_selects_given_global_instance_and_superclass_evidence() {
    let modules = crate::typecheck_program_sources(&[("Main.purs", EVIDENCE_SOURCE)])
        .unwrap_or_else(|errors| panic!("source should type check: {errors:?}"));
    let seen = collect(&modules[0]);
    let mut kinds = BTreeSet::new();
    if seen.given {
        kinds.insert("Given");
    }
    if seen.global {
        kinds.insert("Global");
    }
    if seen.superclass {
        kinds.insert("Superclass");
    }
    if seen.instance {
        kinds.insert("Instance");
    }
    assert_eq!(
        kinds,
        BTreeSet::from(["Given", "Global", "Superclass", "Instance"]),
        "unexpected evidence kinds: {kinds:?}"
    );
}
