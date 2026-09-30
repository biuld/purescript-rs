use super::*;

pub fn typecheck_module(module: hir::Module) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_imports(module, &HashMap::new())
}

/// Type checks a module against the declared types of values it imports from
/// other modules. Imported symbols resolve to their exporting declaration's
/// signature, which the caller reads from the exporting module's HIR.
pub fn typecheck_module_with_imports(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_imports_and_effect_representation(module, imported, false)
}

/// Type checks a trusted embedded library module whose `Effect a` values are
/// implemented as token-taking closures. Ordinary source modules must use
/// [`typecheck_module_with_imports`] so `Effect` remains abstract while
/// unification runs.
pub fn typecheck_module_with_imports_and_effect_representation(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_runtime_representation: bool,
) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_imports_and_effect_context(
        module,
        imported,
        None,
        effect_runtime_representation,
        &[],
        &[],
    )
}

/// Type checks a module with the resolved identity of the library's abstract
/// `Effect` type, including when that identity arrives through transitive value
/// signatures.
///
/// `known_types` is every data, newtype, and synonym declaration in the
/// program. Constructors declared in another module are registered from it so
/// an importer can apply and case on them.
///
/// `imported_instances` are the instance declarations of the modules this one
/// imports, directly or transitively. They become searchable so an instance
/// declared in a dependency can discharge a wanted constraint here.
pub fn typecheck_module_with_imports_and_effect_context(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
    known_types: &[hir::TypeDeclaration],
    imported_instances: &[hir::InstanceDeclaration],
) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_checked_kinds(
        module,
        imported,
        effect_type,
        effect_runtime_representation,
        known_types,
        imported_instances,
        &psrs_kind::CheckedKindEnv::default(),
    )
}

/// Type checks a module against the program's checked kind and role metadata.
/// The environment is keyed by resolved type identity, so imported type
/// constructors retain the role contract of their defining module.
pub fn typecheck_module_with_checked_kinds(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
    known_types: &[hir::TypeDeclaration],
    imported_instances: &[hir::InstanceDeclaration],
    checked_kinds: &psrs_kind::CheckedKindEnv,
) -> Result<thir::Module, Vec<TypeCheckError>> {
    if let Err(errors) = module.verify() {
        return Err(errors
            .into_iter()
            .map(|error| {
                TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    error.span,
                    format!("invalid HIR: {}", error.message),
                )
            })
            .collect());
    }

    let mut checker = Checker::new(
        &module,
        imported,
        effect_type,
        effect_runtime_representation,
        known_types,
        imported_instances,
        checked_kinds,
    );
    let components = order::declaration_order(&module);
    let mut inferred = (0..module.declarations.len())
        .map(|_| None)
        .collect::<Vec<Option<InferredDeclaration>>>();

    for component in &components {
        for &index in component {
            let declaration = &module.declarations[index];
            let (scheme, parameters) = match &declaration.signature {
                Some(signature) => {
                    let (constraints, parameters, body) =
                        checker.elaborate_declaration_signature(signature);
                    (
                        Scheme {
                            variables: Vec::new(),
                            constraints,
                            ty: body,
                        },
                        parameters,
                    )
                }
                None => (Scheme::monomorphic(checker.fresh()), Vec::new()),
            };
            checker
                .pending_signatures
                .insert(declaration.symbol, parameters);
            checker.globals.insert(declaration.symbol, scheme);
        }
        for &index in component {
            let declaration = &module.declarations[index];
            let scheme = checker.globals[&declaration.symbol].clone();
            let parameters = checker
                .pending_signatures
                .get(&declaration.symbol)
                .cloned()
                .unwrap_or_default();
            checker.begin_givens(&scheme.constraints, &parameters);
            let wanted_start = checker.wanted.len();
            let expected = declaration
                .signature
                .as_ref()
                .map(|_| checker.globals[&declaration.symbol].ty.clone());
            let Some(value) = checker.infer_expr_with_expected(&declaration.value, expected) else {
                checker.end_givens();
                continue;
            };
            let span = declaration
                .signature
                .as_ref()
                .map_or(declaration.name_span, |signature| signature.span);
            checker.unify(scheme.ty.clone(), value.ty.clone(), span);
            // An inferred (signatureless) binding is generalized below, so its
            // constraints must be determinate; a declared signature may name
            // ambiguous variables for the caller to instantiate.
            let result = declaration.signature.is_none().then(|| value.ty.clone());
            checker.solve_wanted_constraints(result.as_ref(), wanted_start);
            checker.end_givens();
            let value = checker.wrap_dictionary_lambdas(value, &parameters);
            inferred[index] = Some(InferredDeclaration {
                symbol: declaration.symbol,
                name: declaration.name.clone(),
                name_span: declaration.name_span,
                scheme,
                value,
                span: declaration.span,
            });
        }
        // Generalize after the component is inferred so later components
        // instantiate polymorphic definitions.
        for &index in component {
            let Some((monomorphic, constraints)) = inferred[index].as_ref().map(|declaration| {
                (
                    declaration.scheme.ty.clone(),
                    declaration.scheme.constraints.clone(),
                )
            }) else {
                continue;
            };
            let scheme = checker.generalize(&monomorphic, &constraints, TOP_LEVEL);
            if let Some(declaration) = inferred[index].as_mut() {
                declaration.scheme = scheme.clone();
                checker.globals.insert(declaration.symbol, scheme);
            }
        }
    }

    // Instance dictionaries are ordinary declarations emitted after the value
    // declarations they may reference.
    for instance in &module.instances {
        if let Some(declaration) = checker.infer_instance_declaration(instance) {
            inferred.push(Some(declaration));
        }
    }

    if !checker.errors.is_empty() {
        return Err(checker.errors);
    }
    let inferred = inferred.into_iter().flatten().collect::<Vec<_>>();

    let mut types = TypeInterner::default();
    let mut generics = checker.generic_variables.clone();
    let declarations = inferred
        .into_iter()
        .filter_map(|declaration| {
            let quantified = declaration
                .scheme
                .variables
                .iter()
                .copied()
                .map(TypeVariableId)
                .collect();
            let value = checker.finalize_expr(declaration.value, &mut types, &generics)?;
            Some(thir::Declaration {
                symbol: declaration.symbol,
                name: declaration.name,
                name_span: declaration.name_span,
                quantified,
                ty: value.ty,
                value,
                span: declaration.span,
            })
        })
        .collect::<Vec<_>>();
    if !checker.errors.is_empty() {
        return Err(checker.errors);
    }

    let constructor_infos = checker
        .constructor_info
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let newtype_ids = module
        .types
        .iter()
        .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::Newtype)
        .map(|declaration| declaration.id)
        .collect();
    let opaque_ids = module
        .types
        .iter()
        .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::Foreign)
        .map(|declaration| declaration.id)
        .collect();
    let mut constructors = Vec::with_capacity(constructor_infos.len());
    for info in constructor_infos {
        // Imported constructors are emitted so this module can lower
        // applications and patterns. Linking keeps one copy per symbol.
        let mut variables = HashMap::new();
        let mut parameters = Vec::with_capacity(info.parameters.len());
        for parameter in &info.parameters {
            let variable = checker.fresh();
            if let InferType::Variable(id) = variable {
                generics.insert(id);
                parameters.push(TypeVariableId(id));
            }
            variables.insert(parameter.clone(), variable);
        }
        let Some(field_types) = info
            .fields
            .iter()
            .map(|field| {
                let inferred = checker.elaborate_type(field, &mut variables);
                checker.finalize_type(&inferred, field.span, &mut types, &generics)
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        constructors.push(thir::ConstructorInfo {
            symbol: info.symbol,
            name: info.name.clone(),
            type_id: info.type_id,
            tag: info.tag,
            field_count: field_types.len(),
            field_types,
            parameters,
        });
    }

    let type_names = module
        .types
        .iter()
        .map(|declaration| {
            (
                declaration.id,
                format!("{}.{}", module.name, declaration.name),
            )
        })
        .collect();
    // The trusted elaboration registers the runtime representation of the
    // imported abstract `Effect` type by its resolved type identity: a closure
    // with one hidden context parameter. It is representation metadata, not a
    // type node, and no effect-specific type, flag, or token is introduced.
    let callable_types = effect_type.map(|id| vec![(id, 1)]).unwrap_or_default();
    let typed = thir::Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types: types.values,
        newtype_ids,
        opaque_ids,
        callable_types,
        constructors,
        declarations,
        type_names,
        span: module.span,
    };
    match typed.verify() {
        Ok(()) => Ok(typed),
        Err(errors) => Err(errors
            .into_iter()
            .map(|error| {
                TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    error.span,
                    format!("invalid THIR: {}", error.message),
                )
            })
            .collect()),
    }
}
