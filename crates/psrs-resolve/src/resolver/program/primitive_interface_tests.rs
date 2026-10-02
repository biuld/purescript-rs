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
    ast::Module {
        name: name("Main"),
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
