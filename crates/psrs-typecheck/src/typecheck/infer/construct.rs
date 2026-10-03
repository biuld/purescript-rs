use super::*;

impl Checker {
    pub(in crate::typecheck) fn new(
        module: &hir::Module,
        imported: &HashMap<SymbolId, hir::Type>,
        context: TypecheckContext<'_>,
    ) -> Self {
        let TypecheckContext {
            known_types,
            imported_instances,
            module_names,
            checked_kinds,
        } = context;
        let type_declarations = module
            .types
            .iter()
            .chain(known_types.iter())
            .map(|declaration| (declaration.id, declaration.clone()))
            .collect::<HashMap<_, _>>();
        let visible_newtypes = type_declarations
            .values()
            .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::Newtype)
            .filter_map(|declaration| {
                let constructor = declaration.constructors.first()?;
                let visible_locally = declaration.id.module == module.id;
                let visible_through_import = module.imports.iter().any(|import| {
                    import
                        .types
                        .iter()
                        .any(|ty| ty.reference == hir::TypeReference::Named(declaration.id))
                        && import
                            .symbols
                            .iter()
                            .any(|symbol| symbol.symbol == constructor.symbol)
                });
                (visible_locally || visible_through_import).then_some(declaration.id)
            })
            .collect();
        let mut type_modules = module
            .types
            .iter()
            .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::Class)
            .map(|declaration| (declaration.id, module.name.clone()))
            .collect::<HashMap<_, _>>();
        for declaration in known_types {
            if declaration.kind == hir::TypeDeclarationKind::Class
                && let Some(name) = module_names.get(&declaration.id.module)
            {
                type_modules.insert(declaration.id, name.clone());
            }
        }
        for import in &module.imports {
            for ty in &import.types {
                if let hir::TypeReference::Named(id) = ty.reference {
                    type_modules
                        .entry(id)
                        .or_insert_with(|| import.module_name.clone());
                }
            }
        }
        let mut checker = Self {
            env: SemanticEnv {
                module_id: module.id,
                external_kinds: module
                    .externals
                    .iter()
                    .map(|external| (external.symbol, external.kind.clone()))
                    .collect(),
                external_signatures: module
                    .externals
                    .iter()
                    .filter_map(|external| {
                        external
                            .signature
                            .clone()
                            .map(|signature| (external.symbol, signature))
                    })
                    .collect(),
                imported: imported.clone(),
                type_names: module
                    .types
                    .iter()
                    .map(|declaration| (declaration.id, declaration.name.clone()))
                    .collect(),
                type_modules,
                type_declarations,
                visible_newtypes,
                synonyms: module
                    .types
                    .iter()
                    .chain(known_types.iter())
                    .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::TypeSynonym)
                    .filter_map(|declaration| {
                        let body = declaration.body.clone()?;
                        Some((
                            declaration.id,
                            Synonym {
                                parameters: declaration
                                    .parameters
                                    .iter()
                                    .map(|parameter| parameter.name.clone())
                                    .collect(),
                                body,
                            },
                        ))
                    })
                    .collect(),
                checked_kinds: checked_kinds.clone(),
                constructor_info: module
                    .types
                    .iter()
                    .flat_map(|declaration| {
                        let parameters = declaration
                            .parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect::<Vec<_>>();
                        declaration
                            .constructors
                            .iter()
                            .enumerate()
                            .map(|(tag, constructor)| {
                                (
                                    constructor.symbol,
                                    ConstructorInfo {
                                        symbol: constructor.symbol,
                                        name: constructor.name.clone(),
                                        type_id: declaration.id,
                                        tag: tag as u32,
                                        parameters: parameters.clone(),
                                        fields: constructor.fields.clone(),
                                    },
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect(),
                classes: HashMap::new(),
                class_methods: HashMap::new(),
                instances: Vec::new(),
            },
            state: InferState {
                substitutions: HashMap::new(),
                levels: HashMap::new(),
                rigid: HashSet::new(),
                generic_variables: HashSet::new(),
                variable_kinds: HashMap::new(),
                kinds: psrs_kind::KindState::default(),
                wanted: Vec::new(),
                next_wanted_id: 0,
                errors: Vec::new(),
                warnings: Vec::new(),
                reported_fundep_conflicts: HashSet::new(),
                expanding: HashSet::new(),
                pending_signatures: HashMap::new(),
                next_variable: 0,
                next_dictionary_local: 0,
                level: 1,
            },
            scope: Scope::default(),
        };
        checker.import_known_types(known_types);
        // Constructor field types may contain nested class constraints, so
        // class identities must be registered before those field schemes are
        // elaborated. Both environments are built from the complete module
        // declarations, which also supports forward and mutually referring
        // class/data declarations.
        checker.build_class_environment(module, known_types);
        checker.register_constructors();
        checker.state.next_dictionary_local = classes::next_local_id(module);
        checker.build_instance_environment(module, imported_instances);
        checker
    }
}
