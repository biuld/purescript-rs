//! Fidelity tests for the `Prim.*` registry against the official primitive
//! environment. Every assertion here reads the shared declarations the registry
//! publishes; none of them reproduces an official member in a private table.

use super::*;

/// The official non-builtin member list, transcribed from
/// `Language.PureScript.Constants.Prim`'s `primModules`. The twelve
/// `Prim`-owned built-in types are not registry declarations: they keep their
/// `BuiltinType` identities, and the resolver's `PRIM_TYPES` table owns them.
const OFFICIAL_MEMBERS: &[(&str, &str)] = &[
    ("Prim", "Partial"),
    ("Prim.Boolean", "False"),
    ("Prim.Boolean", "True"),
    ("Prim.Coerce", "Coercible"),
    ("Prim.Int", "Add"),
    ("Prim.Int", "Compare"),
    ("Prim.Int", "Mul"),
    ("Prim.Int", "ToString"),
    ("Prim.Ordering", "Ordering"),
    ("Prim.Ordering", "LT"),
    ("Prim.Ordering", "EQ"),
    ("Prim.Ordering", "GT"),
    ("Prim.Row", "Cons"),
    ("Prim.Row", "Lacks"),
    ("Prim.Row", "Nub"),
    ("Prim.Row", "Union"),
    ("Prim.RowList", "RowList"),
    ("Prim.RowList", "RowToList"),
    ("Prim.RowList", "Cons"),
    ("Prim.RowList", "Nil"),
    ("Prim.Symbol", "Append"),
    ("Prim.Symbol", "Compare"),
    ("Prim.Symbol", "Cons"),
    ("Prim.TypeError", "Fail"),
    ("Prim.TypeError", "Warn"),
    ("Prim.TypeError", "Above"),
    ("Prim.TypeError", "Beside"),
    ("Prim.TypeError", "Doc"),
    ("Prim.TypeError", "Quote"),
    ("Prim.TypeError", "QuoteLabel"),
    ("Prim.TypeError", "Text"),
];

fn declared(owner: &str, name: &str) -> TypeDeclaration {
    primitive_type_declarations()
        .into_iter()
        .find(|(module, declaration)| *module == owner && declaration.name == name)
        .unwrap_or_else(|| panic!("{owner}.{name} is not declared by the registry"))
        .1
}

#[test]
fn every_official_primitive_member_is_declared_exactly_once() {
    let mut declared = primitive_type_declarations()
        .into_iter()
        .map(|(owner, declaration)| (owner.to_owned(), declaration.name.clone()))
        .collect::<Vec<_>>();
    declared.sort();
    let mut official = OFFICIAL_MEMBERS
        .iter()
        .map(|(owner, name)| ((*owner).to_owned(), (*name).to_owned()))
        .collect::<Vec<_>>();
    official.sort();
    assert_eq!(
        declared, official,
        "the registry must declare exactly the official `Prim` members"
    );
}

#[test]
fn boolean_literals_are_declared_at_the_boolean_kind() {
    // The official environment registers `True` and `False` at the type
    // `Boolean`, which is what makes them type-level boolean literals rather
    // than ordinary types.
    for name in ["True", "False"] {
        assert_eq!(
            declared("Prim.Boolean", name)
                .declared_kind
                .as_ref()
                .map(|kind| &kind.kind),
            Some(&TypeKind::Constructor(BuiltinType::Boolean)),
            "Prim.Boolean.{name} must be declared at kind `Boolean`"
        );
    }
}

#[test]
fn the_registry_declares_no_rules_or_diagnostics() {
    for (_, declaration) in primitive_type_declarations() {
        assert!(
            declaration.body.is_none(),
            "`{}` is a declaration, not a synonym body",
            declaration.name
        );
        assert!(
            declaration.constructors.is_empty(),
            "`{}` is a foreign or class declaration, not a data one",
            declaration.name
        );
    }
}
