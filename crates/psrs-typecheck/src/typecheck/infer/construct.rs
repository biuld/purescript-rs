use super::*;

impl Checker {
    pub(in crate::typecheck) fn new(
        module: &hir::Module,
        imported: &HashMap<SymbolId, hir::Type>,
        effect_type: Option<hir::TypeId>,
        effect_runtime_representation: bool,
        known_types: &[hir::TypeDeclaration],
        imported_instances: &[hir::InstanceDeclaration],
        checked_kinds: &psrs_kind::CheckedKindEnv,
    ) -> Self {
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
                    import.types.iter().any(|ty| ty.id == declaration.id)
                        && import
                            .symbols
                            .iter()
                            .any(|symbol| symbol.symbol == constructor.symbol)
                });
                (visible_locally || visible_through_import).then_some(declaration.id)
            })
            .collect();
        let mut checker = Self {
            module_id: module.id,
            globals: HashMap::new(),
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
            locals: HashMap::new(),
            type_names: module
                .types
                .iter()
                .map(|declaration| (declaration.id, declaration.name.clone()))
                .collect(),
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
            // The driver supplies this identity only for its embedded Prelude.
            // A module name or imported type name is not enough to establish trust.
            effect_type,
            effect_runtime_representation,
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
            expanding: HashSet::new(),
            substitutions: HashMap::new(),
            levels: HashMap::new(),
            generic_variables: HashSet::new(),
            rigid: HashSet::new(),
            next_variable: 0,
            level: 1,
            classes: HashMap::new(),
            class_methods: HashMap::new(),
            instances: Vec::new(),
            pending_signatures: HashMap::new(),
            givens: Vec::new(),
            wanted: Vec::new(),
            next_dictionary_local: 0,
            reported_fundep_conflicts: HashSet::new(),
            errors: Vec::new(),
        };
        checker.import_known_types(known_types);
        checker.register_constructors();
        checker.next_dictionary_local = classes::next_local_id(module);
        checker.build_class_environment(module, known_types);
        checker.build_instance_environment(module, imported_instances);
        checker
    }
}
