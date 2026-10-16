use super::ResolveErrorKind;
use super::names::Resolver;
use super::names::builtin_type;
use psrs_ast as ast;
use psrs_hir::{
    self as hir, ExportedInstance, ExportedOperator, ExportedSymbol, ExportedType,
    ExportedTypeOperator, ModuleId, SymbolId, TypeDeclarationKind, TypeId, TypeReference,
};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod import_scope;
mod instance_visibility;
mod transitive;
use instance_visibility::instance_is_public;

#[derive(Default)]
struct ExportAccumulator {
    values: Vec<ExportedSymbol>,
    operators: Vec<ExportedOperator>,
    types: Vec<ExportedType>,
    type_operators: Vec<ExportedTypeOperator>,
    value_sources: HashMap<String, ModuleId>,
    type_sources: HashMap<String, TypeReference>,
}

struct TypeExportRequest {
    reference: TypeReference,
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
        module_id: ModuleId,
        types: &[hir::TypeDeclaration],
        declarations: &[hir::Declaration],
        instances: &[hir::InstanceDeclaration],
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
                        Some(reference) => {
                            let declaration_id = match reference {
                                TypeReference::Named(id) => Some(id),
                                TypeReference::Builtin(_) => None,
                            };
                            let declaration = declaration_id.and_then(|id| own.get(&id).copied());
                            let constructors =
                                self.export_members(declaration, members.as_ref(), &name);
                            let opaque = declaration.is_some_and(|declaration| {
                                declaration.kind == TypeDeclarationKind::Foreign
                            }) || declaration_id
                                .is_some_and(|id| self.imported_opaque(id));
                            self.add_type(
                                &mut exports,
                                TypeExportRequest {
                                    reference,
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
                        let hir::FixityTarget::Type(reference) = fixity.target else {
                            unreachable!("type fixities always target type declarations")
                        };
                        self.add_type_operator(
                            &mut exports.type_operators,
                            &mut exports.type_sources,
                            reference,
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
        self.check_transitive_exports(
            types,
            declarations,
            &exports.types,
            &exports.values,
            &exports.operators,
            &exports.type_operators,
        );
        let exported_type_ids: HashSet<TypeId> = exports
            .types
            .iter()
            .filter_map(|exported| match exported.reference {
                TypeReference::Named(id) => Some(id),
                TypeReference::Builtin(_) => None,
            })
            .collect();
        let exported_instances = instances
            .iter()
            .filter(|instance| instance_is_public(instance, module_id, &exported_type_ids))
            .map(|instance| ExportedInstance {
                symbol: instance.symbol,
                name: instance.name.clone(),
                name_span: instance.name_span,
            })
            .collect();
        Some(hir::ExportList {
            values: exports.values,
            operators: exports.operators,
            types: exports.types,
            type_operators: exports.type_operators,
            instances: exported_instances,
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
        sources: &mut HashMap<String, TypeReference>,
        reference: TypeReference,
        name: String,
        target_name: String,
        span: TextRange,
    ) {
        match sources.get(&name) {
            Some(existing) if *existing != reference => {
                self.report(ResolveErrorKind::ExportConflict, name, span);
            }
            Some(_) => {}
            None => {
                sources.insert(name.clone(), reference);
                operators.push(ExportedTypeOperator {
                    reference,
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
            Some(existing) if *existing != request.reference => {
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
                    .insert(request.name.clone(), request.reference);
                exports.types.push(ExportedType {
                    reference: request.reference,
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
        if matches.is_empty() {
            self.report(
                ResolveErrorKind::UnknownExport,
                name.text.clone(),
                name.span,
            );
            return;
        }
        // A shared import alias (for example the standard library's
        // `as Exports`) denotes the union of those modules' exports.
        if matches.len() > 1 && !self.check_reexport_scope(name, &matches) {
            return;
        }
        for import in &matches {
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
                    hir::FixityTarget::Type(reference) => self.add_type_operator(
                        &mut exports.type_operators,
                        &mut exports.type_sources,
                        reference,
                        fixity.operator.clone(),
                        fixity.target_name.clone(),
                        name.span,
                    ),
                }
            }
            for imported in &import.types {
                if import.fixities.iter().any(|fixity| {
                    fixity.namespace == hir::FixityNamespace::Type
                        && fixity.operator == imported.name
                }) {
                    continue;
                }
                self.add_type(
                    exports,
                    TypeExportRequest {
                        reference: imported.reference,
                        name: imported.name.clone(),
                        name_span: name.span,
                        is_class: false,
                        constructors: (!imported.constructors.is_empty()).then(|| {
                            imported
                                .constructors
                                .iter()
                                .map(|(_, symbol)| *symbol)
                                .collect()
                        }),
                        opaque: imported.opaque,
                    },
                );
            }
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
        // `Format()` exports the type and none of its constructors.
        if members.names.is_empty() {
            return Some(Vec::new());
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
        if let Some(reference) = self
            .imported_types
            .get(name)
            .and_then(|references| references.first().copied())
        {
            return Some(reference);
        }
        // `Unit` is a compiler builtin with no local declaration. `Data.Unit`
        // re-exports that builtin so `import Data.Unit (Unit)` is the same type.
        builtin_type(name).map(TypeReference::Builtin)
    }
}
