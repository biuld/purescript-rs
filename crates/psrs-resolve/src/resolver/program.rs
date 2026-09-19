use super::{
    ModuleInputs, ResolveError, ResolveErrorKind, bootstrap_externals, resolve_ast_module,
};
use psrs_ast as ast;
use psrs_hir::{self as hir, ImportedSymbol, ImportedType, ModuleId, SymbolId, TypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

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
        let Some(target) = registry.get(&import.module.text).copied() else {
            continue;
        };
        let interface = interfaces[target].as_ref();
        imports.push(build_import(
            module_index,
            import,
            target,
            interface,
            errors,
        ));
    }
    imports
}

fn build_import(
    module_index: usize,
    import: &ast::Import,
    target: usize,
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
                for (name, id) in &interface.types {
                    types.push(imported_type(*id, name, import.span));
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
                for (name, id) in &interface.types {
                    if !hidden.contains(name) {
                        types.push(imported_type(*id, name, import.span));
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
                            let Some(id) = interface.types.get(&name.text).copied() else {
                                unknown_import(module_index, name, errors);
                                continue;
                            };
                            types.push(imported_type(id, &name.text, name.span));
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
                                Some(id) => {
                                    types.push(imported_type(id, &name.text, name.span));
                                    for (member, symbol) in interface
                                        .class_members
                                        .get(&name.text)
                                        .into_iter()
                                        .flatten()
                                    {
                                        symbols.push(imported(*symbol, member, member, name.span));
                                    }
                                }
                                None => unknown_import(module_index, name, errors),
                            }
                        }
                        ast::ImportRef::Module(_) => {}
                    }
                }
            }
        }
    }
    hir::Import {
        module: ModuleId(target as u32),
        module_name: import.module.text.clone(),
        alias: import.alias.as_ref().map(|alias| alias.text.clone()),
        hiding: import.list.as_ref().is_some_and(|list| list.hiding),
        symbols,
        types,
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

fn imported_type(id: TypeId, name: &str, span: TextRange) -> ImportedType {
    ImportedType {
        id,
        name: name.to_string(),
        span,
    }
}

/// The namespaces a resolved module exposes to its importers.
struct Interface {
    values: HashMap<String, SymbolId>,
    types: HashMap<String, TypeId>,
    /// Exported data constructors per type name.
    constructors: HashMap<String, Vec<(String, SymbolId)>>,
    /// Exported class members per class name.
    class_members: HashMap<String, Vec<(String, SymbolId)>>,
}

impl Interface {
    fn from_module(module: &hir::Module) -> Self {
        let mut values = HashMap::new();
        let mut types = HashMap::new();
        let mut constructors = HashMap::new();
        let mut class_members = HashMap::new();
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
                        let members = declaration
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
        }
    }
}
