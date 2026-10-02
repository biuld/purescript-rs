use super::names::Resolver;
use super::*;

pub(crate) fn resolve_ast_module(
    module: ast::Module,
    module_id: ModuleId,
    inputs: ModuleInputs,
) -> Result<hir::Module, Vec<ResolveError>> {
    if is_prim_module(&module.name.text) {
        return Err(vec![ResolveError::named(
            ResolveErrorKind::CannotDefinePrimModules,
            module.name.text,
            module.name.span,
        )]);
    }

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
    let role_declarations: HashMap<String, ast::RoleDeclaration> = module
        .role_declarations
        .into_iter()
        .map(|declaration| (declaration.name.text.clone(), declaration))
        .collect();
    let (plans, type_names, opaque_types, mut next_symbol) = plan_type_declarations(
        module_id,
        &type_declarations,
        module.declarations.len() as u32,
        &mut globals,
        &mut errors,
    );

    let mut external_globals = HashMap::new();
    for external in &inputs.externals {
        if matches!(
            &external.kind,
            ExternalKind::Intrinsic(hir::Intrinsic::Coerce)
        ) {
            continue;
        }
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
        Vec::new(),
        errors,
    );
    let local_fixities = resolver.resolve_fixity_declarations(&module.fixities);
    resolver.opaque_types.extend(opaque_types);
    resolver.note_imported_opaque_types();

    // A `foreign import` declares an external value whose type and WIT binding
    // come from source. Resolve its annotation first so expressions can refer
    // to it by name.
    for (index, foreign) in module.foreign_imports.iter().enumerate() {
        let binding = foreign
            .binding
            .split_once('#')
            .filter(|(interface, function)| !interface.is_empty() && !function.is_empty());
        let Some((interface, function)) = binding else {
            resolver.errors.push(ResolveError {
                kind: ResolveErrorKind::InvalidHir,
                span: foreign.span,
                message: format!(
                    "`{}` is not a `\"<interface>#<function>\"` WIT binding",
                    foreign.binding
                ),
            });
            continue;
        };
        let Some(signature) = resolver.resolve_type(foreign.annotation.clone()) else {
            continue;
        };
        let symbol = foreign_symbol(module_id, index);
        resolver.add_external(
            foreign.name.text.clone(),
            symbol,
            ExternalSymbol {
                symbol,
                name: foreign.name.text.clone(),
                kind: ExternalKind::Wit {
                    interface: interface.to_string(),
                    function: function.to_string(),
                },
                signature: Some(signature),
            },
            foreign.name.span,
        );
    }

    let declarations: Vec<hir::Declaration> = module
        .declarations
        .into_iter()
        .enumerate()
        .filter_map(|(index, declaration)| {
            let declaration_name = declaration.name.text.clone();
            let declaration_span = declaration.span;
            let errors_before_value = resolver.errors.len();
            let Some(value) = resolver.resolve_expr(declaration.value) else {
                if resolver.errors.len() == errors_before_value {
                    resolver.errors.push(ResolveError::resolution_failure(
                        declaration_span,
                        &format!("value declaration `{declaration_name}`"),
                    ));
                }
                return None;
            };
            let errors_before_signature = resolver.errors.len();
            let signature = match declaration.annotation {
                Some(annotation) => match resolver.resolve_type(annotation) {
                    Some(signature) => Some(signature),
                    None => {
                        if resolver.errors.len() == errors_before_signature {
                            resolver.errors.push(ResolveError::resolution_failure(
                                declaration_span,
                                &format!("type signature for `{declaration_name}`"),
                            ));
                        }
                        return None;
                    }
                },
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
        .filter_map(|(declaration, plan)| {
            let role = role_declarations.get(&declaration.name().text).cloned();
            resolver.resolve_type_declaration(plan, declaration, role)
        })
        .collect();
    let exports = resolver.build_exports(&types, &declarations);
    let instances: Vec<hir::InstanceDeclaration> = module
        .instances
        .into_iter()
        .enumerate()
        .filter_map(|(index, instance)| {
            instances::resolve_instance(&mut resolver, module_id, index, instance, &mut next_symbol)
        })
        .collect();

    if resolver.errors.is_empty() {
        let resolved = hir::Module {
            id: module_id,
            name: module.name.text,
            externals: resolver.externals,
            imports: resolver.imports,
            exports,
            declarations,
            types,
            instances,
            fixities: local_fixities,
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

fn is_prim_module(name: &str) -> bool {
    name == "Prim" || name.starts_with("Prim.")
}
