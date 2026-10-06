use super::*;

#[test]
fn differential_symbol_reflection_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    let root = psrs_driver::standard_library_info()
        .unwrap()
        .root
        .join("lib");
    let proxy = std::fs::read_to_string(root.join("Type/Proxy.purs")).unwrap();
    let symbol = std::fs::read_to_string(root.join("Data/Symbol.purs")).unwrap();
    let main = r#"module Main where
import Data.Symbol as S
import Type.Proxy (Proxy(..))
reflect :: forall s. S.IsSymbol s => Proxy s -> String
reflect proxy = S.reflectSymbol proxy
main :: String
main = reflect (Proxy :: Proxy "λ😀")
"#;
    let sources = [
        ("Proxy.purs", proxy.as_str()),
        ("Symbol.purs", symbol.as_str()),
        ("Main.purs", main),
    ];
    // Official FFI implementation: purescript-prelude v6.0.1, f4cad0ae8106185c9ab407f43cf9abf05c256af4.
    let official = purs_sources_with_foreign_output(
        "symbol-reflection",
        &sources,
        &[(
            "Symbol.js",
            include_str!("../fixtures/upstream-prelude/Symbol.js"),
        )],
    );
    assert!(official.status.success(), "{official:?}");
    psrs_driver::check_program(&sources).expect("accepts official symbol reflection");
}
