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
    let mut module_names = HashMap::from([(module.id, module.name.clone())]);
    for import in &module.imports {
        module_names
            .entry(import.module)
            .or_insert_with(|| import.module_name.clone());
    }
    typecheck_module_with_checked_kinds_and_module_names(
        module,
        imported,
        effect_type,
        effect_runtime_representation,
        TypecheckContext {
            known_types,
            imported_instances,
            module_names: &module_names,
            checked_kinds,
        },
    )
}

/// Type checks a module with the resolved name of every module in its program.
/// Re-exported type identities use their declaring module name, so compiler
/// rules that depend on canonical declaration identity remain stable through
/// umbrella modules.
pub fn typecheck_module_with_checked_kinds_and_module_names(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
    context: TypecheckContext<'_>,
) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_checked_kinds_and_module_names_and_warnings(
        module,
        imported,
        effect_type,
        effect_runtime_representation,
        context,
    )
    .map(|output| output.module)
}

/// Type checks a module and returns warnings separately from errors. The
/// caller owns their source attribution because it knows the module's position
/// in the containing program.
pub fn typecheck_module_with_checked_kinds_and_module_names_and_warnings(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
    context: TypecheckContext<'_>,
) -> Result<TypeCheckOutput, Vec<TypeCheckError>> {
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

    let _ = (effect_type, effect_runtime_representation);
    let mut checker = Checker::new(&module, imported, context);
    let mut inferred = (0..module.declarations.len())
        .map(|_| None)
        .collect::<Vec<Option<InferredDeclaration>>>();
    checker.infer_declarations(&module, &mut inferred);

    // Instance dictionaries are ordinary declarations emitted after the value
    // declarations they may reference.
    for instance in &module.instances {
        if let Some(declaration) = checker.infer_instance_declaration(instance) {
            inferred.push(Some(declaration));
        }
    }

    super::checked_exports::check(&module, &inferred, &mut checker.state.errors);

    if !checker.state.errors.is_empty() {
        return Err(checker.state.errors);
    }
    let inferred = inferred.into_iter().flatten().collect::<Vec<_>>();
    let mut types = TypeInterner::default();
    let mut generics = checker.state.generic_variables.clone();
    let external_types = module
        .externals
        .iter()
        .filter_map(|external| {
            if !matches!(external.kind, hir::ExternalKind::Wit { .. }) {
                return None;
            }
            let signature = external.signature.as_ref()?;
            let inferred = checker.elaborate_type_mode(signature, &mut HashMap::new(), true);
            if contains_constraint(&inferred) {
                checker.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    signature.span,
                    "class-constrained WIT imports are not supported by the WASI binding ABI",
                ));
                return None;
            }
            let ty = checker.finalize_type(&inferred, signature.span, &mut types, &generics)?;
            Some(thir::ExternalType {
                symbol: external.symbol,
                source_module: module.id,
                ty,
            })
        })
        .collect::<Vec<_>>();
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
    if !checker.state.errors.is_empty() {
        return Err(checker.state.errors);
    }

    let constructor_infos = checker
        .env
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
    // Effect stays an ordinary type application through this module. Its
    // closure representation is chosen later, by identity, in one lowering
    // pass. This table is empty.
    let callable_types = Vec::new();
    let typed = thir::Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        external_types,
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
        Ok(()) => Ok(TypeCheckOutput {
            module: typed,
            warnings: checker.state.warnings,
        }),
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

fn contains_constraint(ty: &InferType) -> bool {
    match ty {
        InferType::Constrained { .. } => true,
        InferType::Application(function, argument) => {
            contains_constraint(function) || contains_constraint(argument)
        }
        InferType::ForAll { body, .. } => contains_constraint(body),
        InferType::RowExtend { ty, tail, .. } => {
            contains_constraint(ty) || contains_constraint(tail)
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => false,
    }
}
