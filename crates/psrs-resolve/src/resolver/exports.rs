use super::ResolveErrorKind;
use super::names::Resolver;
use psrs_ast as ast;
use psrs_hir::{
    self as hir, ExportedSymbol, ExportedType, ModuleId, SymbolId, TypeDeclarationKind, TypeId,
};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

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
        let mut values = Vec::new();
        let mut exported_types = Vec::new();
        let mut value_sources: HashMap<String, ModuleId> = HashMap::new();
        let mut type_sources: HashMap<String, ModuleId> = HashMap::new();
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
                        Some(id) => {
                            let declaration = own.get(&id).copied();
                            let constructors =
                                self.export_members(declaration, members.as_ref(), &name);
                            self.add_type(
                                &mut exported_types,
                                &mut type_sources,
                                id,
                                name.text,
                                name.span,
                                declaration.is_some_and(|declaration| {
                                    declaration.kind == TypeDeclarationKind::Class
                                }),
                                constructors,
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
        self.check_transitive_exports(types, &exported_types, &values);
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
        sources: &mut HashMap<String, ModuleId>,
        id: TypeId,
        name: String,
        name_span: TextRange,
        is_class: bool,
        constructors: Option<Vec<SymbolId>>,
    ) {
        match sources.get(&name) {
            Some(existing) if *existing != id.module => {
                self.report(ResolveErrorKind::ExportConflict, name, name_span);
            }
            Some(_) => {}
            None => {
                sources.insert(name.clone(), id.module);
                exported_types.push(ExportedType {
                    id,
                    name,
                    name_span,
                    constructors,
                    is_class,
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
        type_sources: &mut HashMap<String, ModuleId>,
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
                imported.id,
                imported.name.clone(),
                name.span,
                false,
                None,
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

    fn lookup_export(&self, name: &str) -> Option<SymbolId> {
        if let Some(symbol) = self.globals.get(name) {
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

    /// Reports in-module types and class members referenced by an exported
    /// declaration but not exported themselves.
    fn check_transitive_exports(
        &mut self,
        types: &[hir::TypeDeclaration],
        exported_types: &[ExportedType],
        values: &[ExportedSymbol],
    ) {
        let own: HashMap<TypeId, &hir::TypeDeclaration> = types
            .iter()
            .map(|declaration| (declaration.id, declaration))
            .collect();
        let exported_type_ids: HashSet<TypeId> =
            exported_types.iter().map(|exported| exported.id).collect();
        let exported_symbols: HashSet<SymbolId> = values.iter().map(|value| value.symbol).collect();

        // A class member may only be exported together with its class.
        let mut member_owner: HashMap<SymbolId, (&hir::TypeDeclaration, String)> = HashMap::new();
        for declaration in types {
            if declaration.kind != TypeDeclarationKind::Class {
                continue;
            }
            for member in &declaration.members {
                member_owner.insert(member.symbol, (declaration, declaration.name.clone()));
            }
        }
        let mut reported_classes = HashSet::new();
        for value in values {
            if let Some((declaration, class_name)) = member_owner.get(&value.symbol)
                && !exported_type_ids.contains(&declaration.id)
                && reported_classes.insert(class_name.clone())
            {
                self.report(
                    ResolveErrorKind::TransitiveExportError,
                    class_name.clone(),
                    value.span,
                );
            }
        }

        for exported in exported_types {
            let Some(declaration) = own.get(&exported.id).copied() else {
                continue;
            };
            let mut referenced = Vec::new();
            for constructor in &declaration.constructors {
                for field in &constructor.fields {
                    collect_named_types(field, &mut referenced);
                }
            }
            if let Some(body) = &declaration.body {
                collect_named_types(body, &mut referenced);
            }
            for superclass in &declaration.superclasses {
                collect_named_types(superclass, &mut referenced);
            }
            for member in &declaration.members {
                if let Some(signature) = &member.signature {
                    collect_named_types(signature, &mut referenced);
                }
            }

            let mut missing: Vec<String> = Vec::new();
            for id in referenced {
                if exported_type_ids.contains(&id) {
                    continue;
                }
                if let Some(referenced) = own.get(&id)
                    && !missing.contains(&referenced.name)
                {
                    missing.push(referenced.name.clone());
                }
            }
            if declaration.kind == TypeDeclarationKind::Class {
                for member in &declaration.members {
                    if !exported_symbols.contains(&member.symbol) && !missing.contains(&member.name)
                    {
                        missing.push(member.name.clone());
                    }
                }
            }
            for name in missing {
                self.report(
                    ResolveErrorKind::TransitiveExportError,
                    name,
                    exported.name_span,
                );
            }
        }
    }
}

fn collect_named_types(ty: &hir::Type, out: &mut Vec<TypeId>) {
    match &ty.kind {
        hir::TypeKind::Named(id) => out.push(*id),
        hir::TypeKind::Application(function, argument) => {
            collect_named_types(function, out);
            collect_named_types(argument, out);
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_named_types(parameter, out);
            collect_named_types(result, out);
        }
        hir::TypeKind::Forall { variables, body } => {
            for variable in variables {
                if let Some(kind) = &variable.kind {
                    collect_named_types(kind, out);
                }
            }
            collect_named_types(body, out);
        }
        hir::TypeKind::Constrained { constraint, body } => {
            collect_named_types(constraint, out);
            collect_named_types(body, out);
        }
        hir::TypeKind::Row { fields, tail } | hir::TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_named_types(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_named_types(tail, out);
            }
        }
        hir::TypeKind::Variable(_)
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
}
