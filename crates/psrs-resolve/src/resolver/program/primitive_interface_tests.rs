use super::resolve_program;
use psrs_ast as ast;
use psrs_hir::{ModuleId, TypeDeclarationKind, TypeReference};
use psrs_span::TextRange;
use std::collections::{BTreeMap, HashMap, HashSet};

fn name(text: &str) -> ast::Name {
    ast::Name {
        text: text.to_owned(),
        span: TextRange::new(0, text.len() as u32),
    }
}

fn import(module: &str) -> ast::Import {
    ast::Import {
        module: name(module),
        alias: None,
        list: None,
        span: TextRange::new(0, 5),
    }
}

fn import_list(module: &str, items: Vec<ast::ImportRef>) -> ast::Import {
    let span = TextRange::new(0, 20);
    ast::Import {
        module: name(module),
        alias: None,
        list: Some(ast::ImportList {
            hiding: false,
            items,
            span,
        }),
        span,
    }
}

fn module(imports: Vec<ast::Import>) -> ast::Module {
    named_module("Main", imports)
}

fn named_module(module_name: &str, imports: Vec<ast::Import>) -> ast::Module {
    ast::Module {
        name: name(module_name),
        exports: None,
        imports,
        declarations: Vec::new(),
        foreign_imports: Vec::new(),
        type_declarations: Vec::new(),
        role_declarations: Vec::new(),
        fixities: Vec::new(),
        instances: Vec::new(),
        span: TextRange::new(0, 200),
    }
}

#[test]
fn official_primitive_interfaces_use_shared_type_ids() {
    let declarations = psrs_hir::primitive_type_declarations();
    let mut imported_by_module = BTreeMap::<&str, Vec<ast::ImportRef>>::new();
    for (owner, declaration) in &declarations {
        if *owner == "Prim.TypeError" {
            continue;
        }
        let item = if declaration.kind == TypeDeclarationKind::Class {
            ast::ImportRef::Class(name(&declaration.name))
        } else {
            ast::ImportRef::Type {
                name: name(&declaration.name),
                members: None,
            }
        };
        imported_by_module.entry(owner).or_default().push(item);
    }
    // Prim.Number is not a child module; the root Prim interface keeps the
    // existing built-in Int identity alongside its shared Partial class.
    imported_by_module
        .entry("Prim")
        .or_default()
        .push(ast::ImportRef::Type {
            name: name("Int"),
            members: None,
        });
    let mut imports = imported_by_module
        .into_iter()
        .map(|(owner, items)| import_list(owner, items))
        .collect::<Vec<_>>();
    // A wildcard import must expose every registered TypeError member.
    imports.push(import("Prim.TypeError"));

    let resolved = resolve_program(vec![module(imports)]).unwrap();
    let imported = resolved[0]
        .imports
        .iter()
        .flat_map(|import| {
            import.types.iter().map(move |item| {
                (
                    (import.module_name.as_str(), item.name.as_str()),
                    item.reference,
                )
            })
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(
        imported.get(&("Prim", "Int")),
        Some(&TypeReference::Builtin(psrs_hir::BuiltinType::Int))
    );
    for (owner, declaration) in declarations {
        assert_eq!(
            imported.get(&(owner, declaration.name.as_str())),
            Some(&TypeReference::Named(declaration.id)),
            "{owner}.{} must keep its shared primitive TypeId",
            declaration.name
        );
    }
}

#[test]
fn root_prim_interface_exports_partial_in_the_type_namespace() {
    // The official environment registers `Partial` both as a type of kind
    // `Constraint` and as a parameterless class. Both entries carry the same
    // name and the same kind, so the registry's one declaration reaches a type
    // position and a constraint position through the one interface the
    // resolver derives from it.
    let as_type = named_module(
        "AsType",
        vec![import_list(
            "Prim",
            vec![ast::ImportRef::Type {
                name: name("Partial"),
                members: None,
            }],
        )],
    );
    let as_class = named_module(
        "AsClass",
        vec![import_list(
            "Prim",
            vec![ast::ImportRef::Class(name("Partial"))],
        )],
    );

    let resolved = resolve_program(vec![as_type, as_class]).unwrap();
    for module in &resolved {
        assert_eq!(
            imported_types(module).get("Partial"),
            Some(&TypeReference::Named(psrs_hir::TypeId::PRIM_PARTIAL)),
            "a type import and a class import of `Partial` must reach one identity"
        );
    }
}

#[test]
fn every_primitive_member_keeps_one_identity_through_qualification_and_re_export() {
    // The invariant in `prim.md`: a qualified import, an alias, and a re-export
    // of a member all reach the same `TypeId`. Checked on `Partial`, whose type
    // and class readings share one name, and on members of two child modules.
    let aliased = |module: &str, alias: &str| {
        let mut import = import(module);
        import.alias = Some(name(alias));
        import
    };
    let facade = ast::Module {
        name: name("Facade"),
        exports: Some(ast::ExportList {
            items: vec![
                ast::ExportRef::Module(name("P")),
                ast::ExportRef::Module(name("TE")),
                ast::ExportRef::Module(name("R")),
            ],
            span: TextRange::new(0, 20),
        }),
        imports: vec![
            aliased("Prim", "P"),
            aliased("Prim.TypeError", "TE"),
            aliased("Prim.Row", "R"),
        ],
        declarations: Vec::new(),
        foreign_imports: Vec::new(),
        type_declarations: Vec::new(),
        role_declarations: Vec::new(),
        fixities: Vec::new(),
        instances: Vec::new(),
        span: TextRange::new(0, 200),
    };
    let mut facade_import = import("Facade");
    facade_import.alias = Some(name("F"));
    let main = module(vec![
        facade_import,
        import_list("Prim", vec![ast::ImportRef::Class(name("Partial"))]),
        import_list(
            "Prim.TypeError",
            vec![
                ast::ImportRef::Type {
                    name: name("Doc"),
                    members: None,
                },
                ast::ImportRef::Type {
                    name: name("Text"),
                    members: None,
                },
            ],
        ),
        import_list(
            "Prim.RowList",
            vec![ast::ImportRef::Type {
                name: name("RowList"),
                members: None,
            }],
        ),
        import_list("Prim.Row", vec![ast::ImportRef::Class(name("Cons"))]),
    ]);

    let resolved = resolve_program(vec![facade, main]).unwrap();
    let main = &resolved[1];
    // The facade re-exports each child module under its own alias, so a
    // re-exported member and a directly imported member of the same name reach
    // the same declaration.
    let direct = imported_types(main);
    for (member, id) in [
        ("Partial", psrs_hir::TypeId::PRIM_PARTIAL),
        ("Doc", psrs_hir::TypeId::PRIM_TYPE_ERROR_DOC),
        ("Text", psrs_hir::TypeId::PRIM_TYPE_ERROR_TEXT),
        ("RowList", psrs_hir::TypeId::PRIM_ROW_LIST),
        ("Cons", psrs_hir::TypeId::PRIM_ROW_CONS),
    ] {
        assert_eq!(
            direct.get(member),
            Some(&TypeReference::Named(id)),
            "`{member}` must keep its shared primitive TypeId when imported directly"
        );
    }
    let qualified = qualified_types(main);
    for (spelling, id) in [
        ("F.Partial", psrs_hir::TypeId::PRIM_PARTIAL),
        ("F.Doc", psrs_hir::TypeId::PRIM_TYPE_ERROR_DOC),
        ("F.Text", psrs_hir::TypeId::PRIM_TYPE_ERROR_TEXT),
        ("F.Cons", psrs_hir::TypeId::PRIM_ROW_CONS),
    ] {
        assert_eq!(
            qualified.get(spelling),
            Some(&TypeReference::Named(id)),
            "`{spelling}` must keep the shared primitive TypeId through an alias and a re-export"
        );
    }
}

fn imported_types(module: &psrs_hir::Module) -> HashMap<&str, TypeReference> {
    module
        .imports
        .iter()
        .flat_map(|import| {
            import
                .types
                .iter()
                .map(|item| (item.name.as_str(), item.reference))
        })
        .collect()
}

fn qualified_types(module: &psrs_hir::Module) -> BTreeMap<String, TypeReference> {
    module
        .imports
        .iter()
        .filter_map(|import| {
            let qualifier = import.alias.as_deref()?;
            Some(
                import
                    .types
                    .iter()
                    .map(|item| (format!("{qualifier}.{}", item.name), item.reference))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

#[test]
fn the_root_prim_interface_exports_undefined_with_a_stable_identity() {
    // `undefined` is the one value the official root `Prim` module exports, so
    // it has to reach source through the interface the resolver derives from
    // the registry rather than as a free bootstrap name.
    let resolved = resolve_program(vec![module(vec![import("Prim")])]).unwrap();
    let imported = imported_values(&resolved[0]);
    assert_eq!(
        imported.get("undefined"),
        Some(&psrs_hir::Intrinsic::Undefined.symbol()),
        "`Prim.undefined` must keep one compiler-owned identity"
    );
    assert_eq!(
        imported.get("__psrs_undefined"),
        None,
        "the bootstrap spelling must stay out of the value namespace"
    );
}

#[test]
fn undefined_keeps_one_identity_through_an_alias_and_a_re_export() {
    let mut aliased = import("Prim");
    aliased.alias = Some(name("P"));
    let facade = ast::Module {
        name: name("Facade"),
        exports: Some(ast::ExportList {
            items: vec![ast::ExportRef::Module(name("P"))],
            span: TextRange::new(0, 20),
        }),
        imports: vec![aliased],
        declarations: Vec::new(),
        foreign_imports: Vec::new(),
        type_declarations: Vec::new(),
        role_declarations: Vec::new(),
        fixities: Vec::new(),
        instances: Vec::new(),
        span: TextRange::new(0, 200),
    };
    let mut facade_import = import("Facade");
    facade_import.alias = Some(name("F"));
    let main = module(vec![facade_import]);

    let resolved = resolve_program(vec![facade, main]).unwrap();
    assert_eq!(
        imported_values(&resolved[1]).get("undefined"),
        Some(&psrs_hir::Intrinsic::Undefined.symbol()),
        "a re-exported value must keep the compiler-owned identity"
    );
    let qualified = qualified_values(&resolved[1]);
    assert_eq!(
        qualified.get("F.undefined"),
        Some(&psrs_hir::Intrinsic::Undefined.symbol()),
        "a qualified member must keep the compiler-owned identity"
    );
}

fn imported_values(module: &psrs_hir::Module) -> HashMap<&str, psrs_hir::SymbolId> {
    module
        .imports
        .iter()
        .flat_map(|import| {
            import
                .symbols
                .iter()
                .map(|item| (item.local_name.as_str(), item.symbol))
        })
        .collect()
}

fn qualified_values(module: &psrs_hir::Module) -> BTreeMap<String, psrs_hir::SymbolId> {
    module
        .imports
        .iter()
        .filter_map(|import| {
            let qualifier = import.alias.as_deref()?;
            Some(
                import
                    .symbols
                    .iter()
                    .map(|item| (format!("{qualifier}.{}", item.local_name), item.symbol))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

#[test]
fn prim_number_is_not_a_child_module() {
    let errors = resolve_program(vec![module(vec![import("Prim.Number")])]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| { error.error.kind == super::super::ResolveErrorKind::ModuleNotFound })
    );
}

#[test]
fn primitive_ids_use_the_compiler_owned_module_namespace() {
    let declarations = psrs_hir::primitive_type_declarations();
    let ids = declarations
        .iter()
        .map(|(_, declaration)| declaration.id)
        .collect::<HashSet<_>>();
    assert_eq!(
        ids.len(),
        declarations.len(),
        "primitive TypeIds must be unique"
    );
    for (_, declaration) in declarations {
        assert_eq!(declaration.id.module, ModuleId::INTRINSICS);
    }
}
