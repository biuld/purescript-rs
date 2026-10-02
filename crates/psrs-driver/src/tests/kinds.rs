use super::*;

fn kind_codes(source: &str) -> Vec<&'static str> {
    check_program_kinds_lenient(&[("Main.purs", source)])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

#[test]
fn reports_a_kinds_do_not_unify_error() {
    let codes = kind_codes("module Main where\ndata KindError f a = One f | Two (f a)\n");
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn reports_a_partially_applied_synonym() {
    let codes = kind_codes("module Main where\ntype F x y = x -> y\ntype G x = F x\n");
    assert!(codes.contains(&"PartiallyAppliedSynonym"), "{codes:?}");
}

#[test]
fn reports_an_infinite_kind() {
    let codes = kind_codes("module Main where\ndata F a = F (a a)\n");
    assert!(codes.contains(&"InfiniteKind"), "{codes:?}");
}

#[test]
fn reports_a_type_synonym_cycle() {
    let codes = kind_codes("module Main where\ntype T = T\n");
    assert!(codes.contains(&"CycleInTypeSynonym"), "{codes:?}");
}

#[test]
fn reports_a_kind_declaration_cycle() {
    let codes = kind_codes(
        "module Main where\ndata Foo :: Bar -> Type\ndata Foo a = Foo\ndata Bar :: Foo -> Type\ndata Bar a = Bar\n",
    );
    assert!(codes.contains(&"CycleInKindDeclaration"), "{codes:?}");
}

#[test]
fn reports_an_undefined_type_variable() {
    let codes = kind_codes("module Main where\nfoo :: Array a\nfoo = 1\n");
    assert!(codes.contains(&"UndefinedTypeVariable"), "{codes:?}");
}

#[test]
fn accepts_well_kinded_higher_kinded_declarations() {
    let codes = kind_codes("module Main where\ndata Compose f g a = Compose (f (g a))\n");
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn kindchecks_official_polykind_row_symbol_and_ordering_interfaces() {
    let source = r#"module Main where
import Prim.Row (class Cons)
import Prim.Symbol (class Append, class Compare)
import Prim.Ordering (Ordering, LT, EQ, GT)

rowType :: forall label value tail row. Cons label value tail row => Int
rowType = 0

rowHigherKind
  :: forall label (value :: Type -> Type) (tail :: Row (Type -> Type)) (row :: Row (Type -> Type))
   . Cons label value tail row
  => Int
rowHigherKind = 0

rowSymbolKind
  :: forall label (value :: Symbol) (tail :: Row Symbol) (row :: Row Symbol)
   . Cons label value tail row
  => Int
rowSymbolKind = 0

append :: forall left right result. Append left right result => Int
append = 0

compare :: forall left right. Compare left right LT => Int
compare = 0
"#;
    let errors = check_program_kinds_lenient(&[("Main.purs", source)])
        .err()
        .unwrap_or_default();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn kindchecks_all_official_primitive_class_schemes() {
    let source = r#"module Main where
import Prim (class Partial, Int, Row, Symbol, Type)
import Prim.Boolean (True, False)
import Prim.Coerce (class Coercible)
import Prim.Int (class Add, class Compare, class Mul, class ToString)
import Prim.Ordering (Ordering, LT, EQ, GT)
import Prim.RowList (RowList, Cons, Nil, class RowToList)
import Prim.TypeError

data Proxy :: forall k. k -> Type
data Proxy value = Proxy

partial :: Partial => Int
partial = 0

coercible :: forall k (left :: k) (right :: k). Coercible left right => Int
coercible = 0

add :: forall (left :: Int) (right :: Int) (sum :: Int). Add left right sum => Int
add = 0

multiply :: forall (left :: Int) (right :: Int) (product :: Int). Mul left right product => Int
multiply = 0

compareInt :: forall (left :: Int) (right :: Int). Compare left right LT => Int
compareInt = 0

toString :: forall (value :: Int) (text :: Symbol). ToString value text => Int
toString = 0

rowList :: forall k (row :: Row k) (list :: RowList k). RowToList row list => Int
rowList = 0

rowListNil :: Proxy (Nil :: RowList Type)
rowListNil = Proxy

falseProxy :: Proxy False
falseProxy = Proxy

trueProxy :: Proxy True
trueProxy = Proxy

warn :: forall (doc :: Doc) value. Warn doc => value -> value
warn value = value

failConstraint :: forall (doc :: Doc) value. Fail doc => value -> value
failConstraint value = value

"#;
    let errors = check_program_kinds_lenient(&[("Main.purs", source)])
        .err()
        .unwrap_or_default();
    assert!(errors.is_empty(), "{errors:?}");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .err()
        .unwrap_or_default();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn checks_an_instance_head_against_a_standalone_class_kind_signature() {
    // `failing/StandaloneKindSignatures4.purs`. The signature reaches the class
    // scheme, but nothing applied it to the instance that uses the class, so the
    // head was never checked.
    let codes = kind_codes(concat!(
        "module Main where\n",
        "class To :: forall k. k -> k -> Constraint\n",
        "class To a b | a -> b\n",
        "\n",
        "instance to1 :: To Int \"foo\"\n",
    ));
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn accepts_an_instance_for_a_higher_kinded_class_parameter() {
    // The standard library's own `instance functorFunction :: Functor ((->) r)`.
    // Each head argument is checked against the kind its class declares, so the
    // function constructor applied once is a parameter here rather than a
    // partially applied synonym.
    let codes = kind_codes(concat!(
        "module Main where\n",
        "class Functor f where\n",
        "  map :: forall a b. (a -> b) -> f a -> f b\n",
        "\n",
        "instance functorFunction :: Functor ((->) r) where\n",
        "  map f g = \\value -> f (g value)\n",
    ));
    assert!(codes.is_empty(), "{codes:?}");
}
