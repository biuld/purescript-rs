use psrs_ast as ast;
use psrs_hir::{self as hir, Declaration, ExternalSymbol, ModuleId, SymbolId, TypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod program;

pub use program::{ProgramError, ResolveOptions, resolve_program, resolve_program_with_options};

mod bootstrap;
mod exports;
mod names;
mod type_resolution;

pub use bootstrap::bootstrap_externals;
use names::Resolver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveErrorKind {
    DuplicateDeclaration,
    DuplicateLocalBinding,
    DuplicateExternal,
    UnknownName,
    UnknownTypeName,
    DuplicateModule,
    ModuleNotFound,
    CycleInModules,
    UnknownImport,
    UnknownImportDataConstructor,
    UnknownExport,
    UnknownExportDataConstructor,
    TransitiveExportError,
    TransitiveDctorExportError,
    ExportConflict,
    ScopeConflict,
    DeclConflict,
    InvalidHir,
}

impl ResolveErrorKind {
    /// The official PureScript `errorCode` this diagnostic reports, when one
    /// exists. Internal invariants have no compatible code.
    pub fn error_code(self) -> Option<&'static str> {
        Some(match self {
            Self::DuplicateDeclaration => "DuplicateValueDeclaration",
            Self::DuplicateLocalBinding => "OverlappingNamesInLet",
            Self::UnknownName | Self::UnknownTypeName => "UnknownName",
            Self::DuplicateModule => "DuplicateModule",
            Self::ModuleNotFound => "ModuleNotFound",
            Self::CycleInModules => "CycleInModules",
            Self::UnknownImport => "UnknownImport",
            Self::UnknownImportDataConstructor => "UnknownImportDataConstructor",
            Self::UnknownExport => "UnknownExport",
            Self::UnknownExportDataConstructor => "UnknownExportDataConstructor",
            Self::TransitiveExportError => "TransitiveExportError",
            Self::TransitiveDctorExportError => "TransitiveDctorExportError",
            Self::ExportConflict => "ExportConflict",
            Self::ScopeConflict => "ScopeConflict",
            Self::DeclConflict => "DeclConflict",
            Self::DuplicateExternal | Self::InvalidHir => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveError {
    pub kind: ResolveErrorKind,
    pub span: TextRange,
    message: String,
}

impl ResolveError {
    fn named(kind: ResolveErrorKind, name: String, span: TextRange) -> Self {
        let message = match kind {
            ResolveErrorKind::DuplicateDeclaration => {
                format!("duplicate declaration `{name}`")
            }
            ResolveErrorKind::DuplicateLocalBinding => {
                format!("duplicate local binding `{name}`")
            }
            ResolveErrorKind::DuplicateExternal => {
                format!("duplicate external symbol `{name}`")
            }
            ResolveErrorKind::DuplicateModule => format!("duplicate module `{name}`"),
            ResolveErrorKind::ModuleNotFound => format!("module `{name}` was not found"),
            ResolveErrorKind::CycleInModules => {
                format!("a module cycle was detected involving `{name}`")
            }
            ResolveErrorKind::UnknownImport => {
                format!("`{name}` is not exported by the imported module")
            }
            ResolveErrorKind::UnknownImportDataConstructor => {
                format!("`{name}` is not a data constructor of the imported type")
            }
            ResolveErrorKind::UnknownExport => {
                format!("`{name}` is not declared in this module")
            }
            ResolveErrorKind::UnknownExportDataConstructor => {
                format!("`{name}` is not a data constructor of the exported type")
            }
            ResolveErrorKind::TransitiveExportError => {
                format!("the export of `{name}` requires another name to be exported")
            }
            ResolveErrorKind::TransitiveDctorExportError => {
                format!("the export of `{name}` requires all of its data constructors")
            }
            ResolveErrorKind::ExportConflict => {
                format!("`{name}` is exported from more than one module")
            }
            ResolveErrorKind::ScopeConflict => {
                format!("`{name}` is in scope from more than one import")
            }
            ResolveErrorKind::DeclConflict => {
                format!("the name `{name}` is declared in more than one type declaration")
            }
            ResolveErrorKind::UnknownName => format!("unknown name `{name}`"),
            ResolveErrorKind::UnknownTypeName => format!("unknown type name `{name}`"),
            ResolveErrorKind::InvalidHir => format!("invalid resolved HIR: {name}"),
        };
        Self {
            kind,
            span,
            message,
        }
    }

    fn invalid_hir(span: TextRange, detail: &str) -> Self {
        Self {
            kind: ResolveErrorKind::InvalidHir,
            span,
            message: format!("resolved HIR invariant failed: {detail}"),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// The official PureScript `errorCode` for this diagnostic, if any.
    pub fn error_code(&self) -> Option<&'static str> {
        self.kind.error_code()
    }
}

/// The environment a module resolves against, assembled by the program loader.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModuleInputs {
    pub externals: Vec<ExternalSymbol>,
    pub imports: Vec<hir::Import>,
    pub export_items: Option<ast::ExportList>,
}

/// Resolves local and same-module value names in the currently supported AST.
/// Module IDs are assigned by the caller so a future module loader can own them.
pub fn resolve_module(
    module: ast::Module,
    module_id: ModuleId,
) -> Result<hir::Module, Vec<ResolveError>> {
    resolve_module_with_externals(module, module_id, &[])
}

/// Resolves a single module using an explicit set of known external values.
/// Imports are not resolved here; use [`resolve_program`] for a module graph.
pub fn resolve_module_with_externals(
    module: ast::Module,
    module_id: ModuleId,
    externals: &[ExternalSymbol],
) -> Result<hir::Module, Vec<ResolveError>> {
    resolve_ast_module(
        module,
        module_id,
        ModuleInputs {
            externals: externals.to_vec(),
            imports: Vec::new(),
            export_items: None,
        },
    )
}

pub(crate) fn resolve_ast_module(
    module: ast::Module,
    module_id: ModuleId,
    inputs: ModuleInputs,
) -> Result<hir::Module, Vec<ResolveError>> {
    let mut globals = HashMap::new();
    let mut errors = Vec::new();

    for (index, declaration) in module.declarations.iter().enumerate() {
        let symbol = SymbolId::new(module_id, symbol_index(index));
        if globals
            .insert(declaration.name.text.clone(), symbol)
            .is_some()
        {
            errors.push(ResolveError::named(
                ResolveErrorKind::DuplicateDeclaration,
                declaration.name.text.clone(),
                declaration.name.span,
            ));
        }
    }

    let mut type_declarations = module.type_declarations;
    let (plans, type_names) = plan_type_declarations(
        module_id,
        &type_declarations,
        module.declarations.len() as u32,
        &mut globals,
        &mut errors,
    );

    let mut external_globals = HashMap::new();
    for external in &inputs.externals {
        if external_globals
            .insert(external.name.clone(), external.symbol)
            .is_some()
        {
            errors.push(ResolveError::named(
                ResolveErrorKind::DuplicateExternal,
                external.name.clone(),
                module.span,
            ));
        }
    }

    let mut resolver = Resolver::new(
        globals,
        external_globals,
        type_names,
        inputs.externals,
        inputs.imports,
        inputs.export_items,
        errors,
    );
    let declarations = module
        .declarations
        .into_iter()
        .enumerate()
        .filter_map(|(index, declaration)| {
            let value = resolver.resolve_expr(declaration.value)?;
            let signature = match declaration.annotation {
                Some(annotation) => Some(resolver.resolve_type(annotation)?),
                None => None,
            };
            Some(Declaration {
                symbol: SymbolId::new(module_id, symbol_index(index)),
                name: declaration.name.text,
                name_span: declaration.name.span,
                value,
                signature,
                span: declaration.span,
            })
        })
        .collect();
    let types: Vec<hir::TypeDeclaration> = type_declarations
        .drain(..)
        .zip(plans)
        .filter_map(|(declaration, plan)| resolver.resolve_type_declaration(plan, declaration))
        .collect();
    let exports = resolver.build_exports(&types);

    if resolver.errors.is_empty() {
        let resolved = hir::Module {
            id: module_id,
            name: module.name.text,
            externals: resolver.externals,
            imports: resolver.imports,
            exports,
            declarations,
            types,
            span: module.span,
        };
        match resolved.verify() {
            Ok(()) => Ok(resolved),
            Err(invariant_errors) => Err(invariant_errors
                .into_iter()
                .map(|error| ResolveError::invalid_hir(error.span, error.message))
                .collect()),
        }
    } else {
        Err(resolver.errors)
    }
}

/// The allocated IDs for one type declaration, aligned with its constructors
/// and members.
struct PlannedType {
    id: TypeId,
    constructors: Vec<SymbolId>,
    members: Vec<SymbolId>,
}

/// Allocates IDs and symbols for a module's type declarations and reports
/// conflicts in the shared uppercase namespace (`DeclConflict`), matching the
/// order `purs` checks: constructors against earlier declarations first, then
/// the type name, so `data T = T` is allowed but a second `data T` is not.
fn plan_type_declarations(
    module_id: ModuleId,
    declarations: &[ast::TypeDeclaration],
    first_symbol: u32,
    globals: &mut HashMap<String, SymbolId>,
    errors: &mut Vec<ResolveError>,
) -> (Vec<PlannedType>, HashMap<String, TypeId>) {
    let mut plans = Vec::with_capacity(declarations.len());
    let mut type_names = HashMap::new();
    let mut uppercase: HashSet<String> = HashSet::new();
    let mut next_symbol = first_symbol;

    for (index, declaration) in declarations.iter().enumerate() {
        let id = TypeId::new(module_id, symbol_index(index));
        let name = declaration.name();

        let mut constructor_symbols = Vec::new();
        let mut constructor_names = HashSet::new();
        for constructor in type_constructors(declaration) {
            if !constructor_names.insert(constructor.name.text.clone())
                || uppercase.contains(&constructor.name.text)
            {
                errors.push(ResolveError::named(
                    ResolveErrorKind::DeclConflict,
                    constructor.name.text.clone(),
                    constructor.name.span,
                ));
            }
            let symbol = SymbolId::new(module_id, next_symbol);
            next_symbol += 1;
            constructor_symbols.push(symbol);
            globals
                .entry(constructor.name.text.clone())
                .or_insert(symbol);
        }

        if uppercase.contains(&name.text) {
            errors.push(ResolveError::named(
                ResolveErrorKind::DeclConflict,
                name.text.clone(),
                name.span,
            ));
        }
        uppercase.insert(name.text.clone());
        for constructor in type_constructors(declaration) {
            uppercase.insert(constructor.name.text.clone());
        }
        type_names.insert(name.text.clone(), id);

        let mut member_symbols = Vec::new();
        for member in type_members(declaration) {
            let symbol = SymbolId::new(module_id, next_symbol);
            next_symbol += 1;
            member_symbols.push(symbol);
            globals.entry(member.name.text.clone()).or_insert(symbol);
        }

        plans.push(PlannedType {
            id,
            constructors: constructor_symbols,
            members: member_symbols,
        });
    }

    (plans, type_names)
}

fn type_constructors(declaration: &ast::TypeDeclaration) -> Vec<&ast::DataConstructor> {
    match declaration {
        ast::TypeDeclaration::Data(declaration) => declaration.constructors.iter().collect(),
        ast::TypeDeclaration::Newtype(declaration) => declaration.constructor.iter().collect(),
        _ => Vec::new(),
    }
}

fn type_members(declaration: &ast::TypeDeclaration) -> &[ast::ClassMember] {
    match declaration {
        ast::TypeDeclaration::Class(declaration) => &declaration.members,
        _ => &[],
    }
}

fn symbol_index(index: usize) -> u32 {
    u32::try_from(index).expect("a source module cannot contain more declarations than its range")
}

#[cfg(test)]
mod program_tests;
#[cfg(test)]
mod tests;
