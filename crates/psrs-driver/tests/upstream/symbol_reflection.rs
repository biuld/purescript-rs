use super::*;

#[test]
fn differential_symbol_reflection_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    let proxy = include_str!("../../../../stdlib/lib/Type/Proxy.purs");
    let symbol = include_str!("../../../../stdlib/lib/Data/Symbol.purs");
    let main = r#"module Main where
import Data.Symbol as S
import Type.Proxy (Proxy(..))
reflect :: forall s. S.IsSymbol s => Proxy s -> String
reflect proxy = S.reflectSymbol proxy
main :: String
main = reflect (Proxy :: Proxy "λ😀")
"#;
    let sources = [
        ("Proxy.purs", proxy),
        ("Symbol.purs", symbol),
        ("Main.purs", main),
    ];
    let official = purs_sources_output("symbol-reflection", &sources);
    assert!(official.status.success(), "{official:?}");
    psrs_driver::check_program(&sources).expect("accepts official symbol reflection");
}
