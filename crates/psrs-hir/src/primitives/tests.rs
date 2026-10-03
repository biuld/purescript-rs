//! Fidelity tests for the `Prim.*` registry against the official primitive
//! environment. Every assertion here reads the shared declarations the registry
//! publishes; none of them reproduces an official member in a private table.

use super::*;
use crate::RoleDeclaration;

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

fn roles(declaration: &TypeDeclaration) -> Vec<Role> {
    declaration
        .declared_roles
        .as_ref()
        .unwrap_or_else(|| panic!("`{}` has no role signature", declaration.name))
        .roles
        .iter()
        .map(|(role, _)| *role)
        .collect()
}

#[test]
fn phantom_roles_match_the_official_role_signatures() {
    let phantom = Role::Phantom;
    let expected: &[(&str, &str, &[Role])] = &[
        ("Prim.RowList", "RowList", &[phantom]),
        ("Prim.RowList", "Cons", &[phantom, phantom, phantom]),
        ("Prim.RowList", "Nil", &[]),
        ("Prim.TypeError", "Text", &[phantom]),
        ("Prim.TypeError", "Quote", &[phantom]),
        ("Prim.TypeError", "QuoteLabel", &[phantom]),
        ("Prim.TypeError", "Beside", &[phantom, phantom]),
        ("Prim.TypeError", "Above", &[phantom, phantom]),
    ];
    for (owner, name, expected) in expected {
        assert_eq!(
            roles(&declared(owner, name)),
            *expected,
            "{owner}.{name} must carry the official role signature"
        );
    }
}

#[test]
fn every_parameterless_foreign_type_declares_an_empty_role_signature() {
    for (owner, name) in [
        ("Prim.Boolean", "True"),
        ("Prim.Boolean", "False"),
        ("Prim.Ordering", "Ordering"),
        ("Prim.Ordering", "LT"),
        ("Prim.Ordering", "EQ"),
        ("Prim.Ordering", "GT"),
        ("Prim.TypeError", "Doc"),
    ] {
        assert!(
            roles(&declared(owner, name)).is_empty(),
            "{owner}.{name} takes no parameter, so it has no role"
        );
    }
}

#[test]
fn every_foreign_role_annotation_carries_its_own_range() {
    for (owner, declaration) in primitive_type_declarations() {
        if declaration.kind != TypeDeclarationKind::Foreign {
            continue;
        }
        let RoleDeclaration { roles, span } = declaration
            .declared_roles
            .clone()
            .expect("every registry foreign type declares its roles");
        for (_, role_span) in roles {
            assert!(
                role_span.start <= role_span.end && role_span.end <= span.end,
                "{owner}.{} must keep every role annotation inside its own span",
                declaration.name
            );
        }
    }
}

#[test]
fn partial_is_a_parameterless_constraint_kinded_class() {
    // The official environment registers `Partial` both as a type of kind
    // `Constraint` and as a parameterless class. Both entries carry the kind
    // `Constraint` and the name `Partial`, so the registry's single
    // declaration carries both readings: a class in the class namespace, and a
    // constraint-kinded type that `Interface::primitive_module` exports in the
    // type namespace.
    let declaration = declared("Prim", "Partial");
    assert_eq!(declaration.kind, TypeDeclarationKind::Class);
    assert!(declaration.parameters.is_empty());
    assert!(declaration.members.is_empty());
    assert!(declaration.superclasses.is_empty());
    assert!(declaration.fundeps.is_empty());
    assert_eq!(
        declaration.declared_kind.as_ref().map(|kind| &kind.kind),
        Some(&TypeKind::Constructor(BuiltinType::Constraint)),
        "`Partial` must be declared at kind `Constraint`"
    );
}

#[test]
fn row_list_nil_takes_its_kind_variable_as_an_argument() {
    // The official kind is `forall k. RowList k`, not an arrow chain, so `Nil`
    // takes no parameter at all.
    let declaration = declared("Prim.RowList", "Nil");
    let kind = declaration.declared_kind.as_ref().expect("a declared kind");
    let TypeKind::Forall { variables, body } = &kind.kind else {
        panic!("`Nil` must quantify its kind variable")
    };
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].name, "k");
    let TypeKind::Application(head, argument) = &body.kind else {
        panic!("`Nil`'s kind body must apply `RowList` to the kind variable")
    };
    assert_eq!(head.kind, TypeKind::Named(TypeId::PRIM_ROW_LIST));
    assert_eq!(argument.kind, TypeKind::Variable("k".into()));
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
