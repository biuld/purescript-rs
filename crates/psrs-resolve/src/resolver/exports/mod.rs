use super::ResolveErrorKind;
use super::names::Resolver;
use psrs_ast as ast;
use psrs_hir::{
    self as hir, ExportedSymbol, ExportedType, ModuleId, SymbolId, TypeDeclarationKind, TypeId,
    TypeReference,
};
use psrs_span::TextRange;
use std::collections::HashMap;

mod transitive;

impl Resolver {
    /// Resolves an explicit export list into values, types, and classes, and
    /// reports the export diagnostics that only need names: an unknown export,
    /// an unknown data constructor, a partial constructor list
    /// (`TransitiveDctorExportError`), exported names that come from more than
    /// one module (`ExportConflict`), and references from an exported
    /// declaration to another declaration that is not exported.
    pub(super) fn build_exports(
        &mut self,
        types: &[hir::TypeDeclaration],
        declarations: &[hir::Declaration],
    ) -> Option<hir::ExportList> {
        let items = self.export_items.take()?;
        let own: HashMap<TypeId, &hir::TypeDeclaration> = types
            .iter()
            .map(|declaration| (declaration.id, declaration))
            .collect();
        let mut values = Vec::new();
        let mut exported_types = Vec::new();
        let mut value_sources: HashMap<String, ModuleId> = HashMap::new();
        let mut type_sources: HashMap<String, TypeReference> = HashMap::new();
        for item in items.items {
            match item {
                ast::ExportRef::Value(name) | ast::ExportRef::Operator(name) => {
                    if let Some(symbol) = self.lookup_export(&name.text) {
                        self.add_value(
                            &mut values,
                            &mut value_sources,
                            symbol,
                            name.text,
                            name.span,
                        );
                    } else {
                        self.report(ResolveErrorKind::UnknownExport, name.text, name.span);
                    }
                }
                ast::ExportRef::Type { name, members } => {
                    match self.lookup_export_type(&name.text) {
                        Some(reference) => {
                            let declaration = match reference {
                                TypeReference::Builtin(_) => None,
                                TypeReference::Named(id) => own.get(&id).copied(),
                            };
                            let constructors =
                                self.export_members(declaration, members.as_ref(), &name);
                            let opaque = declaration.is_some_and(|declaration| {
                                declaration.kind == TypeDeclarationKind::Foreign
                            }) || match reference {
                                TypeReference::Builtin(_) => false,
                                TypeReference::Named(id) => self.imported_opaque(id),
                            };
                            self.add_type(
                                &mut exported_types,
                                &mut type_sources,
                                reference,
                                name.text,
                                name.span,
                                declaration.is_some_and(|declaration| {
                                    declaration.kind == TypeDeclarationKind::Class
                                }),
                                constructors,
                                opaque,
                            );
                        }
                        None => self.report(ResolveErrorKind::UnknownExport, name.text, name.span),
                    }
                }
                ast::ExportRef::Module(name) => {
                    self.reexport_module(
                        &name,
                        &mut values,
                        &mut exported_types,
                        &mut value_sources,
                        &mut type_sources,
                    );
                }
            }
        }
        self.check_transitive_exports(types, declarations, &exported_types, &values);
        Some(hir::ExportList {
            values,
            types: exported_types,
            span: items.span,
        })
    }

    fn add_value(
        &mut self,
        values: &mut Vec<ExportedSymbol>,
        sources: &mut HashMap<String, ModuleId>,
        symbol: SymbolId,
        name: String,
        span: TextRange,
    ) {
        match sources.get(&name) {
            Some(existing) if *existing != symbol.module => {
                self.report(ResolveErrorKind::ExportConflict, name, span);
            }
            Some(_) => {}
            None => {
                sources.insert(name.clone(), symbol.module);
                values.push(ExportedSymbol { symbol, name, span });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_type(
        &mut self,
        exported_types: &mut Vec<ExportedType>,
        sources: &mut HashMap<String, TypeReference>,
        reference: TypeReference,
        name: String,
        name_span: TextRange,
        is_class: bool,
        constructors: Option<Vec<SymbolId>>,
        opaque: bool,
    ) {
        match sources.get(&name) {
            Some(existing) if *existing != reference => {
                self.report(ResolveErrorKind::ExportConflict, name, name_span);
            }
            Some(_) => {}
            None => {
                sources.insert(name.clone(), reference);
                exported_types.push(ExportedType {
                    reference,
                    name,
                    name_span,
                    constructors,
                    is_class,
                    opaque,
                });
            }
        }
    }

    /// Re-exports every name an imported module provides, matching `module X`
    /// in an export list. The qualified name is the import's alias, or the
    /// module name when the import has no alias.
    fn reexport_module(
        &mut self,
        name: &ast::Name,
        values: &mut Vec<ExportedSymbol>,
        exported_types: &mut Vec<ExportedType>,
        value_sources: &mut HashMap<String, ModuleId>,
        type_sources: &mut HashMap<String, TypeReference>,
    ) {
        let matches: Vec<hir::Import> = self
            .imports
            .iter()
            .filter(|import| match &import.alias {
                Some(alias) => alias == &name.text,
                None => import.module_name == name.text,
            })
            .cloned()
            .collect();
        let [import] = matches.as_slice() else {
            if matches.is_empty() {
                self.report(
                    ResolveErrorKind::UnknownExport,
                    name.text.clone(),
                    name.span,
                );
            } else {
                // More than one import provides the qualifier.
                self.report_conflict(name.text.clone(), name.span);
            }
            return;
        };
        let pseudo = import.alias.is_some();
        for symbol in &import.symbols {
            // A real (unaliased) import re-exports names from unqualified
            // scope, so an ambiguous name is a scope conflict.
            if !pseudo
                && self
                    .unqualified
                    .get(&symbol.external_name)
                    .is_some_and(|symbols| symbols.iter().any(|other| *other != symbol.symbol))
            {
                self.report_conflict(symbol.external_name.clone(), name.span);
                continue;
            }
            self.add_value(
                values,
                value_sources,
                symbol.symbol,
                symbol.external_name.clone(),
                name.span,
            );
        }
        for imported in &import.types {
            self.add_type(
                exported_types,
                type_sources,
                imported.reference,
                imported.name.clone(),
                name.span,
                false,
                None,
                imported.opaque,
            );
        }
    }

    fn export_members(
        &mut self,
        declaration: Option<&hir::TypeDeclaration>,
        members: Option<&ast::TypeMembers>,
        name: &ast::Name,
    ) -> Option<Vec<SymbolId>> {
        let members = members?;
        let declaration = declaration?;
        if members.all {
            return Some(
                declaration
                    .constructors
                    .iter()
                    .map(|constructor| constructor.symbol)
                    .collect(),
            );
        }
        let mut symbols = Vec::new();
        for member in &members.names {
            match declaration
                .constructors
                .iter()
                .find(|constructor| constructor.name == member.text)
            {
                Some(constructor) => symbols.push(constructor.symbol),
                None => self.report(
                    ResolveErrorKind::UnknownExportDataConstructor,
                    member.text.clone(),
                    member.span,
                ),
            }
        }
        // An explicit constructor list must name every constructor of the type.
        let missing = declaration
            .constructors
            .iter()
            .any(|constructor| !symbols.contains(&constructor.symbol));
        if missing {
            self.report(
                ResolveErrorKind::TransitiveDctorExportError,
                name.text.clone(),
                name.span,
            );
        }
        Some(symbols)
    }

    fn imported_opaque(&self, id: TypeId) -> bool {
        self.imports.iter().any(|import| {
            import
                .types
                .iter()
                .any(|imported| imported.reference == TypeReference::Named(id) && imported.opaque)
        })
    }

    fn lookup_export(&self, name: &str) -> Option<SymbolId> {
        if let Some(symbol) = self.globals.get(name) {
            return Some(*symbol);
        }
        if let Some(symbol) = self.external_globals.get(name) {
            return Some(*symbol);
        }
        self.unqualified
            .get(name)
            .and_then(|symbols| symbols.first().copied())
    }

    fn lookup_export_type(&self, name: &str) -> Option<TypeReference> {
        if let Some(id) = self.type_names.get(name) {
            return Some(TypeReference::Named(*id));
        }
        self.imported_types
            .get(name)
            .and_then(|ids| ids.first().copied())
    }
}
