use super::ResolveErrorKind;
use super::names::Resolver;
use psrs_ast as ast;
use psrs_hir::{
    self as hir, ExportedOperator, ExportedSymbol, ExportedType, ExportedTypeOperator, ModuleId,
    SymbolId, TypeDeclarationKind, TypeId,
};
use psrs_span::TextRange;
use std::collections::HashMap;

mod transitive;

use transitive::check_transitive_exports;

#[derive(Default)]
struct ExportAccumulator {
    values: Vec<ExportedSymbol>,
    operators: Vec<ExportedOperator>,
    types: Vec<ExportedType>,
    type_operators: Vec<ExportedTypeOperator>,
    value_sources: HashMap<String, ModuleId>,
    type_sources: HashMap<String, ModuleId>,
}

struct TypeExportRequest {
    id: TypeId,
    name: String,
    name_span: TextRange,
    is_class: bool,
    constructors: Option<Vec<SymbolId>>,
    opaque: bool,
}

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
    ) -> Option<hir::ExportList> {
        let items = self.export_items.take()?;
        let own: HashMap<TypeId, &hir::TypeDeclaration> = types
            .iter()
            .map(|declaration| (declaration.id, declaration))
            .collect();
        let mut exports = ExportAccumulator::default();
        for item in items.items {
            match item {
                ast::ExportRef::Value(name) => {
                    if let Some(symbol) = self.lookup_export(&name.text) {
                        self.add_value(
                            &mut exports.values,
                            &mut exports.value_sources,
                            symbol,
                            name.text,
                            name.span,
                        );
                    } else {
                        self.report(ResolveErrorKind::UnknownExport, name.text, name.span);
                    }
                }
                ast::ExportRef::Operator(name) => {
                    if let Some(symbol) = self.lookup_export(&name.text) {
                        let target_name = self
                            .fixities
                            .get(&name.text)
                            .map(|fixity| fixity.target_name.clone())
                            .unwrap_or_else(|| name.text.clone());
                        self.add_operator(
                            &mut exports.operators,
                            &mut exports.value_sources,
                            symbol,
                            name.text,
                            target_name,
                            name.span,
                        );
                    } else {
                        self.report(ResolveErrorKind::UnknownExport, name.text, name.span);
                    }
                }
                ast::ExportRef::Type { name, members } => {
                    match self.lookup_export_type(&name.text) {
                        Some(id) => {
                            let declaration = own.get(&id).copied();
                            let constructors =
                                self.export_members(declaration, members.as_ref(), &name);
                            let opaque = declaration.is_some_and(|declaration| {
                                declaration.kind == TypeDeclarationKind::Foreign
                            }) || self.imported_opaque(id);
                            self.add_type(
                                &mut exports,
                                TypeExportRequest {
                                    id,
                                    name: name.text,
                                    name_span: name.span,
                                    is_class: declaration.is_some_and(|declaration| {
                                        declaration.kind == TypeDeclarationKind::Class
                                    }),
                                    constructors,
                                    opaque,
                                },
                            );
                        }
                        None => self.report(ResolveErrorKind::UnknownExport, name.text, name.span),
                    }
                }
                ast::ExportRef::TypeOperator(name) => {
                    if let Some(fixity) = self.type_fixities.get(&name.text).cloned() {
                        let hir::FixityTarget::Type(id) = fixity.target else {
                            unreachable!("type fixities always target type declarations")
                        };
                        self.add_type_operator(
                            &mut exports.type_operators,
                            &mut exports.type_sources,
                            id,
                            name.text,
                            fixity.target_name,
                            name.span,
                        );
                    } else {
                        self.report(ResolveErrorKind::UnknownExport, name.text, name.span);
                    }
                }
                ast::ExportRef::Module(name) => {
                    self.reexport_module(&name, &mut exports);
                }
            }
        }
        check_transitive_exports(
            self,
            types,
            &exports.types,
            &exports.values,
            &exports.operators,
            &exports.type_operators,
        );
        Some(hir::ExportList {
            values: exports.values,
            operators: exports.operators,
            types: exports.types,
            type_operators: exports.type_operators,
            span: items.span,
        })
    }

    fn add_operator(
        &mut self,
        operators: &mut Vec<ExportedOperator>,
        sources: &mut HashMap<String, ModuleId>,
        symbol: SymbolId,
        name: String,
        target_name: String,
        span: TextRange,
    ) {
        match sources.get(&name) {
            Some(existing) if *existing != symbol.module => {
                self.report(ResolveErrorKind::ExportConflict, name, span);
            }
            Some(_) => {}
            None => {
                sources.insert(name.clone(), symbol.module);
                operators.push(ExportedOperator {
                    symbol,
                    name,
                    target_name,
                    span,
                });
            }
        }
    }

    fn add_type_operator(
        &mut self,
        operators: &mut Vec<ExportedTypeOperator>,
        sources: &mut HashMap<String, ModuleId>,
        id: TypeId,
        name: String,
        target_name: String,
        span: TextRange,
    ) {
        match sources.get(&name) {
            Some(existing) if *existing != id.module => {
                self.report(ResolveErrorKind::ExportConflict, name, span);
            }
            Some(_) => {}
            None => {
                sources.insert(name.clone(), id.module);
                operators.push(ExportedTypeOperator {
                    id,
                    name,
                    target_name,
                    span,
                });
            }
        }
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

    fn add_type(&mut self, exports: &mut ExportAccumulator, request: TypeExportRequest) {
        match exports.type_sources.get(&request.name) {
            Some(existing) if *existing != request.id.module => {
                self.report(
                    ResolveErrorKind::ExportConflict,
                    request.name,
                    request.name_span,
                );
            }
            Some(_) => {}
            None => {
                exports
                    .type_sources
                    .insert(request.name.clone(), request.id.module);
                exports.types.push(ExportedType {
                    id: request.id,
                    name: request.name,
                    name_span: request.name_span,
                    constructors: request.constructors,
                    is_class: request.is_class,
                    opaque: request.opaque,
                });
            }
        }
    }

    /// Re-exports every name an imported module provides, matching `module X`
    /// in an export list. The qualified name is the import's alias, or the
    /// module name when the import has no alias.
    fn reexport_module(&mut self, name: &ast::Name, exports: &mut ExportAccumulator) {
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
            if import.fixities.iter().any(|fixity| {
                fixity.namespace == hir::FixityNamespace::Value
                    && fixity.operator == symbol.external_name
            }) {
                continue;
            }
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
                &mut exports.values,
                &mut exports.value_sources,
                symbol.symbol,
                symbol.external_name.clone(),
                name.span,
            );
        }
        for fixity in &import.fixities {
            match fixity.target {
                hir::FixityTarget::Value(symbol) => self.add_operator(
                    &mut exports.operators,
                    &mut exports.value_sources,
                    symbol,
                    fixity.operator.clone(),
                    fixity.target_name.clone(),
                    name.span,
                ),
                hir::FixityTarget::Type(id) => self.add_type_operator(
                    &mut exports.type_operators,
                    &mut exports.type_sources,
                    id,
                    fixity.operator.clone(),
                    fixity.target_name.clone(),
                    name.span,
                ),
            }
        }
        for imported in &import.types {
            if import.fixities.iter().any(|fixity| {
                fixity.namespace == hir::FixityNamespace::Type && fixity.operator == imported.name
            }) {
                continue;
            }
            self.add_type(
                exports,
                TypeExportRequest {
                    id: imported.id,
                    name: imported.name.clone(),
                    name_span: name.span,
                    is_class: false,
                    constructors: None,
                    opaque: imported.opaque,
                },
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
                .any(|imported| imported.id == id && imported.opaque)
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

    fn lookup_export_type(&self, name: &str) -> Option<TypeId> {
        if let Some(id) = self.type_names.get(name) {
            return Some(*id);
        }
        self.imported_types
            .get(name)
            .and_then(|ids| ids.first().copied())
    }
}
