use psrs_thir::{Evidence, EvidenceKind, Expr, ExprKind};

const PROXY: &str = "module Type.Proxy where\ndata Proxy (s :: Symbol) = Proxy\n";
const SYMBOL: &str = "module Data.Symbol where\nimport Type.Proxy (Proxy)\n\
class IsSymbol (s :: Symbol) where\n  reflectSymbol :: Proxy s -> String\n";

fn program(main: &str) -> Vec<psrs_thir::Module> {
    crate::typecheck_program_sources(&[
        ("Proxy.purs", PROXY),
        ("Symbol.purs", SYMBOL),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("symbol program should check: {errors:?}"))
}

#[test]
fn symbol_reflection_survives_aliases_and_reexports() {
    let bridge = "module Bridge (module S) where\nimport Data.Symbol as S\n";
    let main = r#"module Main where
import Bridge as B
import Type.Proxy (Proxy(..))
main :: String
main = B.reflectSymbol (Proxy :: Proxy "λ😀")
"#;
    crate::check_program(&[
        ("Proxy.purs", PROXY),
        ("Symbol.purs", SYMBOL),
        ("Bridge.purs", bridge),
        ("Main.purs", main),
    ])
    .expect("canonical class identity survives alias and re-export");
}

#[test]
fn a_same_named_user_class_has_no_compiler_dictionary_rule() {
    let ordinary = SYMBOL.replace("module Data.Symbol", "module Ordinary");
    let main = r#"module Main where
import Ordinary
import Type.Proxy (Proxy(..))
main :: String
main = reflectSymbol (Proxy :: Proxy "unicode")
"#;
    let errors = crate::check_program(&[
        ("Proxy.purs", PROXY),
        ("Ordinary.purs", &ordinary),
        ("Main.purs", main),
    ])
    .expect_err("user class needs its own instance");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code.as_deref() == Some("NoInstanceFound")),
        "{errors:?}"
    );
}

#[test]
fn a_malformed_compiler_class_contract_is_rejected() {
    let malformed = SYMBOL.replace("-> String", "-> Int");
    let errors = crate::check_program(&[("Proxy.purs", PROXY), ("Symbol.purs", &malformed)])
        .expect_err("compiler interface must declare the supported method type");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("dictionary contract")),
        "{errors:?}"
    );
}

#[test]
fn an_unknown_symbol_is_not_defaulted() {
    let main = r#"module Main where
import Data.Symbol
import Type.Proxy (Proxy(..))
main :: String
main = reflectSymbol Proxy
"#;
    crate::check_program(&[
        ("Proxy.purs", PROXY),
        ("Symbol.purs", SYMBOL),
        ("Main.purs", main),
    ])
    .expect_err("reflection requires a known symbol or lexical dictionary");
}

#[test]
fn a_symbol_given_is_used_under_a_quantifier() {
    program(
        r#"module Main where
import Data.Symbol
import Type.Proxy (Proxy(..))
reflect :: forall s. IsSymbol s => Proxy s -> String
reflect proxy = reflectSymbol proxy
main :: String
main = reflect (Proxy :: Proxy "")
"#,
    );
}

fn constructed_evidence(expression: &mut Expr) -> Option<&mut Evidence> {
    match &mut expression.kind {
        ExprKind::Evidence(evidence) => Some(evidence),
        ExprKind::Application(function, _) => constructed_evidence(function),
        ExprKind::FieldAccess { expression, .. } => constructed_evidence(expression),
        _ => None,
    }
}

#[test]
fn constructed_dictionaries_are_verified_as_ordinary_terms() {
    let mut modules = program(
        r#"module Main where
import Data.Symbol
import Type.Proxy (Proxy(..))
main :: String
main = reflectSymbol (Proxy :: Proxy "λ😀")
"#,
    );
    let main = modules.last_mut().unwrap();
    let evidence =
        constructed_evidence(&mut main.declarations[0].value).expect("constructed evidence");
    let EvidenceKind::DictionaryValue(value) = &mut evidence.kind else {
        panic!("expected runtime dictionary")
    };
    let ExprKind::Record(fields) = &mut value.kind else {
        panic!("expected dictionary fields")
    };
    let ExprKind::Lambda { binder, body } = &mut fields[0].1.kind else {
        panic!("expected reflection method")
    };
    assert_eq!(body.kind, ExprKind::String("λ😀".into()));
    // A bound proxy cannot serve as the method's String result.
    body.kind = ExprKind::Local(binder.id);
    assert!(
        main.verify().is_err(),
        "method body result disagrees with its binder"
    );
}

#[test]
fn symbol_reflection_returns_canonical_utf8_at_runtime() {
    let source = r#"module Main where
import Data.Symbol (reflectSymbol)
import Type.Proxy (Proxy(..))
main :: Int
main =
  let bytes = stringToBytes (reflectSymbol (Proxy :: Proxy "λ😀"))
  in if intEq (arrayLength bytes) 6 then
       if intEq (arrayIndex bytes 0) 206 then
         if intEq (arrayIndex bytes 1) 187 then
           if intEq (arrayIndex bytes 2) 240 then
             if intEq (arrayIndex bytes 3) 159 then
               if intEq (arrayIndex bytes 4) 152 then
                 if intEq (arrayIndex bytes 5) 128 then 42 else 1
               else 2
             else 3
           else 4
         else 5
       else 6
     else 7
"#;
    let Some(output) = super::run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn compiler_interface_contracts_accept_equivalent_type_synonyms() {
    let symbol = SYMBOL
        .replace(
            "class IsSymbol",
            "type Reflected s = Proxy s -> String\nclass IsSymbol",
        )
        .replace(
            "reflectSymbol :: Proxy s -> String",
            "reflectSymbol :: Reflected s",
        );
    crate::check_program(&[
        ("Proxy.purs", PROXY),
        ("Symbol.purs", &symbol),
        (
            "Main.purs",
            r#"module Main where
import Data.Symbol
import Type.Proxy (Proxy(..))
main :: String
main = reflectSymbol (Proxy :: Proxy "known")
"#,
        ),
    ])
    .expect("semantic interface validation expands synonyms");
}

#[test]
fn constructed_dictionary_methods_cannot_reference_missing_locals() {
    let mut modules = program(
        r#"module Main where
import Data.Symbol
import Type.Proxy (Proxy(..))
main :: String
main = reflectSymbol (Proxy :: Proxy "known")
"#,
    );
    let main = modules.last_mut().unwrap();
    let evidence = constructed_evidence(&mut main.declarations[0].value).unwrap();
    let EvidenceKind::DictionaryValue(value) = &mut evidence.kind else {
        panic!("runtime dictionary")
    };
    let ExprKind::Record(fields) = &mut value.kind else {
        panic!("dictionary record")
    };
    let ExprKind::Lambda { body, .. } = &mut fields[0].1.kind else {
        panic!("method lambda")
    };
    body.kind = ExprKind::Local(psrs_hir::LocalId(u32::MAX));
    assert!(
        main.verify().is_err(),
        "dictionary bodies have ordinary lexical scope"
    );
}
