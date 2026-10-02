use super::*;

/// Converts source-oriented CST nodes into a normalized, unresolved surface AST.
pub fn lower_module(module: cst::Module) -> Result<Module, Vec<LowerError>> {
    let mut errors = Vec::new();
    let mut declarations = Vec::new();
    let mut foreign_imports = Vec::new();
    let mut type_declarations = Vec::new();
    let mut role_declarations = Vec::new();
    let mut fixities = Vec::new();
    let mut instances = Vec::new();
    let mut instance_chains = type_decl::InstanceChainTracker::default();
    let mut index = 0;
    while index < module.declarations.len() {
        let declaration = module.declarations[index].clone();
        if !matches!(
            &declaration,
            cst::Declaration::Instance(_) | cst::Declaration::Derive(_)
        ) {
            instance_chains.reset();
        }
        match declaration {
            cst::Declaration::KindSignature(signature) => {
                if matches_kind_declaration(&signature, module.declarations.get(index + 1)) {
                    let target = module.declarations[index + 1].clone();
                    match type_decl::lower_type_declaration(Some(signature), target) {
                        Ok(declaration) => {
                            let role_count = lower_following_role(
                                &declaration,
                                &module.declarations,
                                index + 2,
                                &mut role_declarations,
                                &mut errors,
                            );
                            type_declarations.push(declaration);
                            index += role_count;
                        }
                        Err(error) => errors.push(error),
                    }
                    index += 2;
                } else {
                    errors.push(LowerError::coded(
                        signature.span,
                        "OrphanKindDeclaration",
                        "a kind declaration must be followed by a matching declaration",
                    ));
                    index += 1;
                }
            }
            cst::Declaration::Data(_)
            | cst::Declaration::Newtype(_)
            | cst::Declaration::TypeSynonym(_)
            | cst::Declaration::Class(_) => {
                match type_decl::lower_type_declaration(None, declaration) {
                    Ok(declaration) => {
                        let role_count = lower_following_role(
                            &declaration,
                            &module.declarations,
                            index + 1,
                            &mut role_declarations,
                            &mut errors,
                        );
                        type_declarations.push(declaration);
                        index += role_count;
                    }
                    Err(error) => errors.push(error),
                }
                index += 1;
            }
            cst::Declaration::Instance(declaration) => {
                let (chain_id, chain_position) = match instance_chains.next(&declaration) {
                    Ok(position) => position,
                    Err(error) => {
                        errors.push(error);
                        index += 1;
                        continue;
                    }
                };
                match instance_decl::lower_instance(declaration, chain_id, chain_position, None) {
                    Ok(instance) => instances.push(instance),
                    Err(error) => {
                        instance_chains.reset();
                        errors.push(error);
                    }
                }
                index += 1;
            }
            cst::Declaration::Derive(declaration) => {
                let chain_id = instance_chains.next_singleton();
                match instance_decl::lower_derive(declaration, chain_id) {
                    Ok(instance) => instances.push(instance),
                    Err(error) => errors.push(error),
                }
                index += 1;
            }
            cst::Declaration::Foreign(declaration) => {
                if declaration.data_keyword_span.is_some() {
                    match type_decl::lower_foreign_data(declaration) {
                        Ok(declaration) => {
                            let role_count = lower_following_role(
                                &declaration,
                                &module.declarations,
                                index + 1,
                                &mut role_declarations,
                                &mut errors,
                            );
                            type_declarations.push(declaration);
                            index += role_count;
                        }
                        Err(error) => errors.push(error),
                    }
                } else {
                    match lower_foreign_import(declaration) {
                        Ok(foreign) => foreign_imports.push(foreign),
                        Err(error) => errors.push(error),
                    }
                }
                index += 1;
            }
            cst::Declaration::Role(declaration) => {
                errors.push(LowerError::coded(
                    declaration.span,
                    "OrphanRoleDeclaration",
                    "a role declaration must immediately follow its type declaration",
                ));
                index += 1;
            }
            cst::Declaration::Fixity(declaration) => {
                match super::fixity::lower_declaration(declaration) {
                    Ok(declaration) => fixities.push(declaration),
                    Err(error) => errors.push(error),
                }
                index += 1;
            }
            other => {
                match lower_declaration(other) {
                    Ok(declaration) => declarations.push(declaration),
                    Err(error) => errors.push(error),
                }
                index += 1;
            }
        }
    }
    if errors.is_empty() {
        Ok(Module {
            name: lower_name(module.name),
            exports: module.exports.map(export::lower_export_list),
            imports: module
                .imports
                .into_iter()
                .map(import::lower_import)
                .collect(),
            declarations,
            foreign_imports,
            type_declarations,
            role_declarations,
            fixities,
            instances,
            span: module.span,
        })
    } else {
        Err(errors)
    }
}

fn lower_following_role(
    declaration: &TypeDeclaration,
    declarations: &[cst::Declaration],
    role_index: usize,
    roles: &mut Vec<RoleDeclaration>,
    errors: &mut Vec<LowerError>,
) -> usize {
    let next = declarations.get(role_index);
    let Some(cst::Declaration::Role(role)) = next else {
        return 0;
    };
    let supported = matches!(
        declaration,
        TypeDeclaration::Data(_) | TypeDeclaration::Newtype(_) | TypeDeclaration::Foreign(_)
    );
    if role.name.text != declaration.name().text {
        errors.push(LowerError::coded(
            role.span,
            "OrphanRoleDeclaration",
            "a role declaration must immediately follow the matching type declaration",
        ));
        return 1;
    }
    if !supported {
        errors.push(LowerError::coded(
            role.span,
            "UnsupportedRoleDeclaration",
            "role declarations are supported only for data, newtype, and foreign data types",
        ));
        return 1;
    }
    let mut annotations = Vec::with_capacity(role.roles.len());
    for name in &role.roles {
        let parsed = match name.text.as_str() {
            "nominal" => TypeRole::Nominal,
            "representational" => TypeRole::Representational,
            "phantom" => TypeRole::Phantom,
            _ => {
                errors.push(LowerError::coded(
                    name.span,
                    "UnknownRole",
                    "a role must be `nominal`, `representational`, or `phantom`",
                ));
                continue;
            }
        };
        annotations.push(RoleAnnotation {
            role: parsed,
            span: name.span,
        });
    }
    roles.push(RoleDeclaration {
        name: lower_name(role.name.clone()),
        roles: annotations,
        span: role.span,
    });
    if let Some(cst::Declaration::Role(duplicate)) = declarations.get(role_index + 1)
        && duplicate.name.text == role.name.text
    {
        errors.push(LowerError::coded(
            duplicate.span,
            "DuplicateRoleDeclaration",
            "a type may have only one role declaration",
        ));
        return 2;
    }
    1
}

/// A kind declaration is matched by the declaration that immediately follows it
/// with the same name and the same declaration keyword, as `purs` requires.
fn matches_kind_declaration(
    signature: &cst::KindSignature,
    next: Option<&cst::Declaration>,
) -> bool {
    let Some(next) = next else {
        return false;
    };
    let name = signature.name.text.as_str();
    match (signature.kind_for, next) {
        (cst::KindFor::Data, cst::Declaration::Data(declaration)) => declaration.name.text == name,
        (cst::KindFor::Newtype, cst::Declaration::Newtype(declaration)) => {
            declaration.name.text == name
        }
        (cst::KindFor::TypeSynonym, cst::Declaration::TypeSynonym(declaration)) => {
            declaration.name.text == name
        }
        (cst::KindFor::Class, cst::Declaration::Class(declaration)) => {
            declaration.name.text == name
        }
        _ => false,
    }
}
