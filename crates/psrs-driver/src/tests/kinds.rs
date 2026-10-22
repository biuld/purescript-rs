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

#[test]
fn polykind_generalization_let_is_a_type_mismatch() {
    // `failing/PolykindGeneralizationLet.purs`. The declaration's own field
    // determines `F`, and each use of `Proxy` instantiates its parameter.
    // The kind pass therefore accepts the program, and the two uses of the
    // generalized `a` are the term mismatch the annotation names.
    let source = include_str!("../../../../tests/upstream/failing/PolykindGeneralizationLet.purs");
    let kinds = kind_codes(source);
    assert!(!kinds.contains(&"KindsDoNotUnify"), "{kinds:?}");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("the two uses of the binding disagree");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(codes.contains(&"TypesDoNotUnify"), "{codes:?}");
}

#[test]
fn a_symbol_argument_does_not_match_a_type_parameter() {
    // `failing/3077.purs`. `TProxy a` records `a :: Type`. Passing
    // `SProxy "apple"` binds that variable to a `Symbol`.
    let source = include_str!("../../../../tests/upstream/failing/3077.purs");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("a Symbol is not a Type");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn an_instantiated_instance_keeps_the_method_kinds() {
    // `failing/PolykindInstantiatedInstance.purs`. `(a -> b)` forces `a :: Type`.
    // `Proxy :: _ "foo"` uses `Proxy` at `Symbol`.
    let source =
        include_str!("../../../../tests/upstream/failing/PolykindInstantiatedInstance.purs");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("Proxy at Symbol does not match a Type argument");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn a_shared_row_argument_cannot_have_two_labels() {
    // `failing/3765-kinds.purs`. `{a :: Int | r}` and `{b :: Int | r}` share
    // `r` and differ in the known label, so `Tricky x x` is one argument with
    // two row kinds.
    let source = include_str!("../../../../tests/upstream/failing/3765-kinds.purs");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("one argument cannot have both row kinds");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn an_ill_scoped_kind_quantifier_is_rejected() {
    // `failing/QuantificationCheckFailure.purs`. Generalizing the unknown in
    // `d`'s kind before `a` is declared mentions `a` too early. The kind pass
    // reports the official code; the type checker sees the same diagnostic.
    let source = include_str!("../../../../tests/upstream/failing/QuantificationCheckFailure.purs");
    let kinds = kind_codes(source);
    assert!(
        kinds.contains(&"QuantificationCheckFailureInKind"),
        "{kinds:?}"
    );
    let errors = crate::check_program_types_lenient_with_prelude(&[("Main.purs", source)])
        .expect_err("the implicit kind variable is not in scope");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(
        codes.contains(&"QuantificationCheckFailureInKind"),
        "{codes:?}"
    );
}

#[test]
fn an_unsolved_kind_in_a_constructor_field_is_rejected() {
    // `failing/QuantificationCheckFailure2.purs`. `Proxy`'s kind argument is
    // still unknown inside `forall a. Proxy a`, and a constructor field cannot
    // generalize it.
    let source =
        include_str!("../../../../tests/upstream/failing/QuantificationCheckFailure2.purs");
    let kinds = kind_codes(source);
    assert!(
        kinds.contains(&"QuantificationCheckFailureInType"),
        "{kinds:?}"
    );
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("the constructor field leaves a kind unknown");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(
        codes.contains(&"QuantificationCheckFailureInType"),
        "{codes:?}"
    );
}

#[test]
fn a_monomorphic_binding_group_rejects_a_mismatched_kind() {
    // `failing/MonoKindDataBindingGroup.purs`. `A` and `B` are one monomorphic
    // group, so `F A` solves `A`'s parameter to `Type` and `A "bad"` conflicts.
    let source = include_str!("../../../../tests/upstream/failing/MonoKindDataBindingGroup.purs");
    let kinds = kind_codes(source);
    assert!(kinds.contains(&"KindsDoNotUnify"), "{kinds:?}");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("A \"bad\" is Symbol where Type is required");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn official_role_annotations_report_their_codes() {
    // Non-FFI `failing` role cases. Lowering reports an orphan, a duplicate,
    // and a role on a class or synonym. Kind checking reports a weakened role
    // and a role vector whose length is not the parameter arity.
    let cases: &[(&str, &str)] = &[
        (
            include_str!("../../../../tests/upstream/failing/CoercibleRoleMismatch1.purs"),
            "RoleMismatch",
        ),
        (
            include_str!("../../../../tests/upstream/failing/CoercibleRoleMismatch2.purs"),
            "RoleMismatch",
        ),
        (
            include_str!("../../../../tests/upstream/failing/CoercibleRoleMismatch3.purs"),
            "RoleMismatch",
        ),
        (
            include_str!("../../../../tests/upstream/failing/CoercibleRoleMismatch4.purs"),
            "RoleMismatch",
        ),
        (
            include_str!("../../../../tests/upstream/failing/CoercibleRoleMismatch5.purs"),
            "RoleMismatch",
        ),
        (
            include_str!("../../../../tests/upstream/failing/RoleDeclarationArityMismatch.purs"),
            "RoleDeclarationArityMismatch",
        ),
        (
            include_str!(
                "../../../../tests/upstream/failing/UnsupportedRoleDeclarationTypeClass.purs"
            ),
            "UnsupportedRoleDeclaration",
        ),
        (
            include_str!(
                "../../../../tests/upstream/failing/UnsupportedRoleDeclarationTypeSynonym.purs"
            ),
            "UnsupportedRoleDeclaration",
        ),
        (
            include_str!("../../../../tests/upstream/failing/OrphanRoleDeclaration1.purs"),
            "OrphanRoleDeclaration",
        ),
        (
            include_str!("../../../../tests/upstream/failing/OrphanRoleDeclaration2.purs"),
            "OrphanRoleDeclaration",
        ),
        (
            include_str!("../../../../tests/upstream/failing/OrphanRoleDeclaration3.purs"),
            "OrphanRoleDeclaration",
        ),
        (
            include_str!("../../../../tests/upstream/failing/DuplicateRoleDeclaration.purs"),
            "DuplicateRoleDeclaration",
        ),
    ];
    for (source, expected) in cases {
        let errors =
            crate::check_program_types_lenient(&[("Main.purs", source)]).expect_err(expected);
        let codes: Vec<_> = errors
            .iter()
            .filter_map(|error| error.diagnostic.code)
            .collect();
        assert!(
            codes.contains(expected),
            "{expected} missing from {codes:?}"
        );
    }
}

#[test]
fn a_quantified_kind_escapes_through_proxy() {
    // `failing/SkolemEscapeKinds.purs`. The kind pass accepts the data
    // declarations and reports `EscapedSkolem` for the synonym body.
    let source = include_str!("../../../../tests/upstream/failing/SkolemEscapeKinds.purs");
    let errors = crate::check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("the quantified kind escapes into the synonym");
    let codes: Vec<_> = errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect();
    assert!(codes.contains(&"EscapedSkolem"), "{codes:?}");
}

#[test]
fn a_kind_signature_keeps_an_applied_constructor() {
    // `purs` 0.15.16 accepts this. The parameter kind stays `Box Type`;
    // `elaborateKind` reducing an arrow is not how a kind signature is stored.
    let source = "module Main where\n\
        data Box a = Box a\n\
        data Wrap :: Box Type -> Type\n\
        data Wrap a = Wrap\n\
        main :: Int\n\
        main = 0\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("an unused parameter may have kind `Box Type`: {errors:?}")
    });
}

#[test]
fn a_synonym_rejects_visible_quantification_in_its_kind() {
    // `failing/QuantificationCheckFailure3.purs`. The kind annotation is kept,
    // so the synonym kind mentions `k` and the shipped checker reports the
    // official code. `foreign import` keeps the file off the L3 board.
    let source =
        include_str!("../../../../tests/upstream/failing/QuantificationCheckFailure3.purs");
    let codes = kind_codes(source);
    assert!(
        codes.contains(&"VisibleQuantificationCheckFailureInType"),
        "{codes:?}"
    );
    assert!(
        !codes.contains(&"QuantificationCheckFailureInType"),
        "the annotation is not an unsolved kind: {codes:?}"
    );
}

#[test]
fn a_kind_signature_application_does_not_reduce_to_type() {
    // The field needs kind `Type`. The parameter kind is still `Box Type`.
    let source = "module Main where\n\
        data Box a = Box a\n\
        data Wrap :: Box Type -> Type\n\
        data Wrap a = Wrap a\n";
    let codes = kind_codes(source);
    assert!(
        codes.contains(&"KindsDoNotUnify"),
        "the parameter kind stays `Box Type`: {codes:?}"
    );
}
