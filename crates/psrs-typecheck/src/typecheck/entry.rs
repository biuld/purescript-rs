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
    )
}

/// Type checks a module with the resolved identity of the library's abstract
/// `Effect` type, including when that identity arrives through transitive value
/// signatures.
///
/// `known_types` is every data, newtype, and synonym declaration in the
/// program. Constructors declared in another module are registered from it so
/// an importer can apply and case on them.
pub fn typecheck_module_with_imports_and_effect_context(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
    known_types: &[hir::TypeDeclaration],
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
    );
    let components = order::declaration_order(&module);
    let mut inferred = (0..module.declarations.len())
        .map(|_| None)
        .collect::<Vec<Option<InferredDeclaration>>>();

    for component in &components {
        for &index in component {
            let declaration = &module.declarations[index];
            let ty = match &declaration.signature {
                Some(signature) => checker.elaborate_signature(signature),
                None => checker.fresh(),
            };
            checker
                .globals
                .insert(declaration.symbol, Scheme::monomorphic(ty));
        }
        for &index in component {
            let declaration = &module.declarations[index];
            let expected = declaration
                .signature
                .as_ref()
                .map(|_| checker.globals[&declaration.symbol].ty.clone());
            let Some(value) = checker.infer_expr_with_expected(&declaration.value, expected) else {
                continue;
            };
            let scheme = checker.globals[&declaration.symbol].clone();
            let span = declaration
                .signature
                .as_ref()
                .map_or(declaration.name_span, |signature| signature.span);
            checker.unify(scheme.ty.clone(), value.ty.clone(), span);
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
            let Some(monomorphic) = inferred[index].as_ref().map(|d| d.scheme.ty.clone()) else {
                continue;
            };
            let scheme = checker.generalize(&monomorphic, TOP_LEVEL);
            if let Some(declaration) = inferred[index].as_mut() {
                declaration.scheme = scheme.clone();
                checker.globals.insert(declaration.symbol, scheme);
            }
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
            let ty = checker.finalize_type(
                &declaration.scheme.ty,
                declaration.name_span,
                &mut types,
                &generics,
            )?;
            let value = checker.finalize_expr(declaration.value, &mut types, &generics)?;
            Some(thir::Declaration {
                symbol: declaration.symbol,
                name: declaration.name,
                name_span: declaration.name_span,
                quantified,
                ty,
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
        for parameter in &info.parameters {
            let variable = checker.fresh();
            if let InferType::Variable(id) = variable {
                generics.insert(id);
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
        });
    }

    let typed = thir::Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types: types.values,
        newtype_ids,
        opaque_ids,
        constructors,
        declarations,
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
