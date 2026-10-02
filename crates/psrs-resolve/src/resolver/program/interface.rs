use psrs_hir::{self as hir, Intrinsic, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

/// The namespaces a resolved module exposes to its importers.
#[derive(Clone)]
pub(super) struct Interface {
    pub(super) values: HashMap<String, SymbolId>,
    pub(super) types: HashMap<String, TypeId>,
    pub(super) value_fixities: HashMap<String, hir::Fixity>,
    pub(super) type_fixities: HashMap<String, hir::Fixity>,
    /// Exported data constructors per type name.
    pub(super) constructors: HashMap<String, Vec<(String, SymbolId)>>,
    /// Exported class members per class name.
    pub(super) class_members: HashMap<String, Vec<(String, SymbolId)>>,
    /// Exported opaque foreign data types.
    pub(super) opaque: HashSet<TypeId>,
}

impl Interface {
    pub(super) fn primitive_module(name: &str) -> Option<Self> {
        let mut interface = Self {
            values: HashMap::new(),
            types: HashMap::new(),
            value_fixities: HashMap::new(),
            type_fixities: HashMap::new(),
            constructors: HashMap::new(),
            class_members: HashMap::new(),
            opaque: HashSet::new(),
        };
        match name {
            "Prim.Coerce" => {
                interface
                    .types
                    .insert("Coercible".to_owned(), TypeId::COERCIBLE);
            }
            "Safe.Coerce" => {
                interface
                    .values
                    .insert("coerce".to_owned(), Intrinsic::Coerce.symbol());
                interface
                    .types
                    .insert("Coercible".to_owned(), TypeId::COERCIBLE);
            }
            _ => return None,
        }
        Some(interface)
    }

    pub(super) fn from_module(module: &hir::Module) -> Self {
        let mut values = HashMap::new();
        let mut types = HashMap::new();
        let mut value_fixities = HashMap::new();
        let mut type_fixities = HashMap::new();
        let mut constructors = HashMap::new();
        let mut class_members = HashMap::new();
        let mut opaque = HashSet::new();
        match &module.exports {
            Some(exports) => {
                for value in &exports.values {
                    values.insert(value.name.clone(), value.symbol);
                }
                for operator in &exports.operators {
                    values.insert(operator.name.clone(), operator.symbol);
                    if let Some(fixity) = find_fixity(module, &operator.name) {
                        value_fixities.insert(operator.name.clone(), fixity);
                    }
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
                for operator in &exports.type_operators {
                    types.insert(operator.name.clone(), operator.id);
                    if let Some(fixity) = find_fixity(module, &operator.name) {
                        type_fixities.insert(operator.name.clone(), fixity);
                    }
                }
            }
            None => {
                for declaration in &module.declarations {
                    values.insert(declaration.name.clone(), declaration.symbol);
                }
                for external in &module.externals {
                    if matches!(external.kind, hir::ExternalKind::Wit { .. }) {
                        values.insert(external.name.clone(), external.symbol);
                    }
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
                for fixity in &module.fixities {
                    match fixity.namespace {
                        hir::FixityNamespace::Value => {
                            if let hir::FixityTarget::Value(symbol) = fixity.target {
                                values.insert(fixity.operator.clone(), symbol);
                                value_fixities.insert(fixity.operator.clone(), fixity.clone());
                            }
                        }
                        hir::FixityNamespace::Type => {
                            if let hir::FixityTarget::Type(id) = fixity.target {
                                types.insert(fixity.operator.clone(), id);
                                type_fixities.insert(fixity.operator.clone(), fixity.clone());
                            }
                        }
                    }
                }
            }
        }
        Self {
            values,
            types,
            value_fixities,
            type_fixities,
            constructors,
            class_members,
            opaque,
        }
    }
}

fn find_fixity(module: &hir::Module, name: &str) -> Option<hir::Fixity> {
    module
        .fixities
        .iter()
        .chain(module.imports.iter().flat_map(|import| &import.fixities))
        .find(|fixity| fixity.operator == name)
        .cloned()
}
