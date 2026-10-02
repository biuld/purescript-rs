use super::{
    ModuleInputs, ResolveError, ResolveErrorKind, bootstrap_externals, resolve_ast_module,
};
use psrs_ast as ast;
use psrs_hir::{self as hir, ImportedSymbol, ImportedType, ModuleId, SymbolId, TypeReference};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod prim_and_negate_tests;
#[cfg(test)]
mod primitive_interface_tests;
#[cfg(test)]
mod tests;

mod interface;

use interface::Interface;

/// A resolution error tied to one module of a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramError {
    pub module: usize,
    pub error: ResolveError,
}

/// Options that control how strictly a program is resolved.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolveOptions {
    /// Continue resolving modules whose imports cannot be found, treating a
    /// missing import as introducing no names. This collects every diagnostic a
    /// module can produce instead of stopping at the first missing import and is
    /// used to measure resolution coverage against the corpus.
    pub tolerate_missing_modules: bool,
}

/// Resolves a whole program: assigns module IDs, builds the import graph, and
/// resolves each module against the interfaces of its dependencies. Modules are
/// returned in input order with IDs equal to their input index.
pub fn resolve_program(modules: Vec<ast::Module>) -> Result<Vec<hir::Module>, Vec<ProgramError>> {
    resolve_program_with_options(modules, ResolveOptions::default())
}

/// Resolves a whole program with explicit [`ResolveOptions`].
pub fn resolve_program_with_options(
    modules: Vec<ast::Module>,
    options: ResolveOptions,
) -> Result<Vec<hir::Module>, Vec<ProgramError>> {
    let (resolved, errors) = resolve_program_partial(modules, options);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(resolved
        .into_iter()
        .map(|module| module.expect("every module in the order was resolved"))
        .collect())
}

/// Resolves as much of a program as possible. Every module that resolves
/// successfully is returned in input order; modules that fail are `None`.
/// Diagnostics for all failures, including tolerated missing modules, are
/// returned so a caller can report them or continue with the resolved subset.
pub fn resolve_program_partial(
    modules: Vec<ast::Module>,
    options: ResolveOptions,
) -> (Vec<Option<hir::Module>>, Vec<ProgramError>) {
    let names: Vec<String> = modules
        .iter()
        .map(|module| module.name.text.clone())
        .collect();
    let mut errors = Vec::new();

    let mut registry: HashMap<String, usize> = HashMap::new();
    for (index, module) in modules.iter().enumerate() {
        if registry.insert(module.name.text.clone(), index).is_some() {
            errors.push(ProgramError {
                module: index,
                error: ResolveError::named(
                    ResolveErrorKind::DuplicateModule,
                    module.name.text.clone(),
                    module.name.span,
                ),
            });
        }
    }
    if !errors.is_empty() {
        return ((0..modules.len()).map(|_| None).collect(), errors);
    }

    let edges = build_edges(&modules, &registry, &mut errors);
    if !options.tolerate_missing_modules && !errors.is_empty() {
        return ((0..modules.len()).map(|_| None).collect(), errors);
    }

    let order = topological_order(&edges, &names, &mut errors);
    if !options.tolerate_missing_modules && !errors.is_empty() {
        return ((0..modules.len()).map(|_| None).collect(), errors);
    }

    let mut ast_modules: Vec<Option<ast::Module>> = modules.into_iter().map(Some).collect();
    let mut interfaces: Vec<Option<Interface>> = (0..ast_modules.len()).map(|_| None).collect();
    let mut resolved: Vec<Option<hir::Module>> = (0..ast_modules.len()).map(|_| None).collect();

    for &index in &order {
        let Some(module) = ast_modules[index].take() else {
            continue;
        };
        let imports = build_imports(index, &module, &registry, &interfaces, &mut errors);
        let inputs = ModuleInputs {
            externals: bootstrap_externals(),
            imports,
            export_items: module.exports.clone(),
        };
        match resolve_ast_module(module, ModuleId(index as u32), inputs) {
            Ok(module) => {
                interfaces[index] = Some(Interface::from_module(&module));
                resolved[index] = Some(module);
            }
            Err(module_errors) => {
                for error in module_errors {
                    errors.push(ProgramError {
                        module: index,
                        error,
                    });
                }
            }
        }
    }

    (resolved, errors)
}

fn build_edges(
    modules: &[ast::Module],
    registry: &HashMap<String, usize>,
    errors: &mut Vec<ProgramError>,
) -> Vec<Vec<(usize, TextRange)>> {
    let mut edges = Vec::with_capacity(modules.len());
    for (index, module) in modules.iter().enumerate() {
        let mut module_edges = Vec::new();
        for import in &module.imports {
            match registry.get(&import.module.text) {
                Some(target) => module_edges.push((*target, import.span)),
                None if Interface::primitive_module(&import.module.text).is_some() => {}
                None => errors.push(ProgramError {
                    module: index,
                    error: ResolveError::named(
                        ResolveErrorKind::ModuleNotFound,
                        import.module.text.clone(),
                        import.module.span,
                    ),
                }),
            }
        }
        edges.push(module_edges);
    }
    edges
}

fn topological_order(
    edges: &[Vec<(usize, TextRange)>],
    names: &[String],
    errors: &mut Vec<ProgramError>,
) -> Vec<usize> {
    let mut state = vec![State::Unvisited; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for index in 0..edges.len() {
        visit(index, edges, names, &mut state, &mut order, errors);
    }
    order
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Unvisited,
    Visiting,
    Done,
}

fn visit(
    index: usize,
    edges: &[Vec<(usize, TextRange)>],
    names: &[String],
    state: &mut [State],
    order: &mut Vec<usize>,
    errors: &mut Vec<ProgramError>,
) {
    match state[index] {
        State::Done => return,
        State::Visiting => return,
        State::Unvisited => {}
    }
    state[index] = State::Visiting;
    for (target, span) in &edges[index] {
        if state[*target] == State::Visiting {
            errors.push(ProgramError {
                module: index,
                error: ResolveError::named(
                    ResolveErrorKind::CycleInModules,
                    names[*target].clone(),
                    *span,
                ),
            });
        } else {
            visit(*target, edges, names, state, order, errors);
        }
    }
    state[index] = State::Done;
    order.push(index);
}

fn build_imports(
    module_index: usize,
    module: &ast::Module,
    registry: &HashMap<String, usize>,
    interfaces: &[Option<Interface>],
    errors: &mut Vec<ProgramError>,
) -> Vec<hir::Import> {
    let mut imports = Vec::with_capacity(module.imports.len());
    for import in &module.imports {
        let (target, interface) = match registry.get(&import.module.text).copied() {
            Some(target) => (ModuleId(target as u32), interfaces[target].clone()),
            None => {
                let Some(interface) = Interface::primitive_module(&import.module.text) else {
                    continue;
                };
                (ModuleId::COMPILER_PRELUDE, Some(interface))
            }
        };
        imports.push(build_import(
            module_index,
            import,
            target,
            interface.as_ref(),
            errors,
        ));
    }
    imports
}

fn build_import(
    module_index: usize,
    import: &ast::Import,
    target: ModuleId,
    interface: Option<&Interface>,
    errors: &mut Vec<ProgramError>,
) -> hir::Import {
    let mut symbols = Vec::new();
    let mut types = Vec::new();
    if let Some(interface) = interface {
        match &import.list {
            None => {
                for (name, symbol) in &interface.values {
                    symbols.push(imported(*symbol, name, name, import.span));
                }
                for (name, reference) in &interface.types {
                    types.push(imported_type(
                        *reference,
                        name,
                        import.span,
                        reference_is_opaque(interface, *reference),
                    ));
                }
            }
            Some(list) if list.hiding => {
                let mut hidden = HashSet::new();
                for item in &list.items {
                    let name = item.name();
                    if !interface.values.contains_key(&name.text)
                        && !interface.types.contains_key(&name.text)
                    {
                        errors.push(ProgramError {
                            module: module_index,
                            error: ResolveError::named(
                                ResolveErrorKind::UnknownImport,
                                name.text.clone(),
                                name.span,
                            ),
                        });
                    }
                    hidden.insert(name.text.clone());
                }
                for (name, symbol) in &interface.values {
                    if !hidden.contains(name) {
                        symbols.push(imported(*symbol, name, name, import.span));
                    }
                }
                for (name, reference) in &interface.types {
                    if !hidden.contains(name) {
                        types.push(imported_type(
                            *reference,
                            name,
                            import.span,
                            reference_is_opaque(interface, *reference),
                        ));
                    }
                }
            }
            Some(list) => {
                for item in &list.items {
                    match item {
                        ast::ImportRef::Value(name) | ast::ImportRef::Operator(name) => {
                            match interface.values.get(&name.text) {
                                Some(symbol) => symbols
                                    .push(imported(*symbol, &name.text, &name.text, name.span)),
                                None => unknown_import(module_index, name, errors),
                            }
                        }
                        ast::ImportRef::Type { name, members } => {
                            let Some(reference) = interface.types.get(&name.text).copied() else {
                                unknown_import(module_index, name, errors);
                                continue;
                            };
                            types.push(imported_type(
                                reference,
                                &name.text,
                                name.span,
                                reference_is_opaque(interface, reference),
                            ));
                            match members {
                                None => {}
                                Some(members) if members.all => {
                                    for (member, symbol) in
                                        interface.constructors.get(&name.text).into_iter().flatten()
                                    {
                                        symbols.push(imported(*symbol, member, member, name.span));
                                    }
                                }
                                Some(members) => {
                                    let available = interface.constructors.get(&name.text);
                                    for member in &members.names {
                                        let found = available.and_then(|constructors| {
                                            constructors
                                                .iter()
                                                .find(|(name, _)| *name == member.text)
                                        });
                                        match found {
                                            Some((name, symbol)) => symbols.push(imported(
                                                *symbol,
                                                name,
                                                name,
                                                member.span,
                                            )),
                                            None => errors.push(ProgramError {
                                                module: module_index,
                                                error: ResolveError::named(
                                                    ResolveErrorKind::UnknownImportDataConstructor,
                                                    member.text.clone(),
                                                    member.span,
                                                ),
                                            }),
                                        }
                                    }
                                }
                            }
                        }
                        ast::ImportRef::Class(name) => {
                            match interface.types.get(&name.text).copied() {
                                Some(TypeReference::Named(id)) => {
                                    types.push(imported_type(
                                        TypeReference::Named(id),
                                        &name.text,
                                        name.span,
                                        interface.opaque.contains(&id),
                                    ));
                                    for (member, symbol) in interface
                                        .class_members
                                        .get(&name.text)
                                        .into_iter()
                                        .flatten()
                                    {
                                        symbols.push(imported(*symbol, member, member, name.span));
                                    }
                                }
                                Some(TypeReference::Builtin(_)) | None => {
                                    unknown_import(module_index, name, errors)
                                }
                            }
                        }
                        ast::ImportRef::TypeOperator(name) => {
                            match interface.types.get(&name.text).copied() {
                                Some(reference)
                                    if interface.type_fixities.contains_key(&name.text) =>
                                {
                                    types.push(imported_type(
                                        reference,
                                        &name.text,
                                        name.span,
                                        reference_is_opaque(interface, reference),
                                    ));
                                }
                                _ => unknown_import(module_index, name, errors),
                            }
                        }
                        ast::ImportRef::Module(_) => {}
                    }
                }
            }
        }
    }
    let imported_names: HashSet<String> = symbols
        .iter()
        .map(|symbol| symbol.local_name.clone())
        .chain(types.iter().map(|ty| ty.name.clone()))
        .collect();
    let fixities = interface
        .into_iter()
        .flat_map(|interface| {
            interface
                .value_fixities
                .iter()
                .chain(interface.type_fixities.iter())
                .filter(|(name, _)| imported_names.contains(*name))
                .map(|(_, fixity)| fixity.clone())
        })
        .collect();
    hir::Import {
        module: target,
        module_name: import.module.text.clone(),
        alias: import.alias.as_ref().map(|alias| alias.text.clone()),
        hiding: import.list.as_ref().is_some_and(|list| list.hiding),
        symbols,
        types,
        fixities,
        span: import.span,
    }
}

fn unknown_import(module_index: usize, name: &ast::Name, errors: &mut Vec<ProgramError>) {
    errors.push(ProgramError {
        module: module_index,
        error: ResolveError::named(
            ResolveErrorKind::UnknownImport,
            name.text.clone(),
            name.span,
        ),
    });
}

fn imported(
    symbol: SymbolId,
    local_name: &str,
    external_name: &str,
    span: TextRange,
) -> ImportedSymbol {
    ImportedSymbol {
        symbol,
        local_name: local_name.to_string(),
        external_name: external_name.to_string(),
        span,
    }
}

fn imported_type(
    reference: TypeReference,
    name: &str,
    span: TextRange,
    opaque: bool,
) -> ImportedType {
    ImportedType {
        reference,
        name: name.to_string(),
        span,
        opaque,
    }
}

fn reference_is_opaque(interface: &Interface, reference: TypeReference) -> bool {
    match reference {
        TypeReference::Builtin(_) => false,
        TypeReference::Named(id) => interface.opaque.contains(&id),
    }
}
