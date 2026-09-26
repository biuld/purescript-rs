use psrs_hir::{self as hir, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

/// The namespaces a resolved module exposes to its importers.
pub(super) struct Interface {
    pub(super) values: HashMap<String, SymbolId>,
    pub(super) types: HashMap<String, TypeId>,
    /// Exported data constructors per type name.
    pub(super) constructors: HashMap<String, Vec<(String, SymbolId)>>,
    /// Exported class members per class name.
    pub(super) class_members: HashMap<String, Vec<(String, SymbolId)>>,
    /// Exported opaque foreign data types.
    pub(super) opaque: HashSet<TypeId>,
}

impl Interface {
    pub(super) fn from_module(module: &hir::Module) -> Self {
        let mut values = HashMap::new();
        let mut types = HashMap::new();
        let mut constructors = HashMap::new();
        let mut class_members = HashMap::new();
        let mut opaque = HashSet::new();
        match &module.exports {
            Some(exports) => {
                for value in &exports.values {
                    values.insert(value.name.clone(), value.symbol);
                }
                let declarations = module
                    .types
                    .iter()
                    .map(|declaration| (declaration.id, declaration))
                    .collect::<HashMap<_, _>>();
                for exported in &exports.types {
                    types.insert(exported.name.clone(), exported.id);
                    if exported.opaque {
                        opaque.insert(exported.id);
                    }
                    let Some(declaration) = declarations.get(&exported.id).copied() else {
                        continue;
                    };
                    if let Some(symbols) = &exported.constructors {
                        let mut members = Vec::new();
                        for symbol in symbols {
                            if let Some(constructor) = declaration
                                .constructors
                                .iter()
                                .find(|constructor| constructor.symbol == *symbol)
                            {
                                values
                                    .entry(constructor.name.clone())
                                    .or_insert(constructor.symbol);
                                members.push((constructor.name.clone(), constructor.symbol));
                            }
                        }
                        constructors.insert(exported.name.clone(), members);
                    }
                    if exported.is_class {
                        let members: Vec<(String, SymbolId)> = declaration
                            .members
                            .iter()
                            .filter(|member| values.values().any(|symbol| *symbol == member.symbol))
                            .map(|member| (member.name.clone(), member.symbol))
                            .collect();
                        class_members.insert(exported.name.clone(), members);
                    }
                }
            }
            None => {
                for declaration in &module.declarations {
                    values.insert(declaration.name.clone(), declaration.symbol);
                }
                for declaration in &module.types {
                    types.insert(declaration.name.clone(), declaration.id);
                    if declaration.kind == hir::TypeDeclarationKind::Foreign {
                        opaque.insert(declaration.id);
                    }
                    for constructor in &declaration.constructors {
                        values.insert(constructor.name.clone(), constructor.symbol);
                        constructors
                            .entry(declaration.name.clone())
                            .or_insert_with(Vec::new)
                            .push((constructor.name.clone(), constructor.symbol));
                    }
                    for member in &declaration.members {
                        values.insert(member.name.clone(), member.symbol);
                        class_members
                            .entry(declaration.name.clone())
                            .or_insert_with(Vec::new)
                            .push((member.name.clone(), member.symbol));
                    }
                }
            }
        }
        Self {
            values,
            types,
            constructors,
            class_members,
            opaque,
        }
    }
}
