use super::super::ResolveErrorKind;
use super::super::names::Resolver;
use psrs_hir::{
    self as hir, ExportedOperator, ExportedSymbol, ExportedType, ExportedTypeOperator, SymbolId,
    TypeDeclarationKind, TypeId,
};
use std::collections::{HashMap, HashSet};

/// Reports declarations that an exported alias or declaration exposes
/// transitively without exporting their names.
pub(super) fn check_transitive_exports(
    resolver: &mut Resolver,
    types: &[hir::TypeDeclaration],
    exported_types: &[ExportedType],
    values: &[ExportedSymbol],
    operators: &[ExportedOperator],
    type_operators: &[ExportedTypeOperator],
) {
    let own: HashMap<TypeId, &hir::TypeDeclaration> = types
        .iter()
        .map(|declaration| (declaration.id, declaration))
        .collect();
    let exported_type_ids: HashSet<TypeId> =
        exported_types.iter().map(|exported| exported.id).collect();
    let exported_symbols: HashSet<SymbolId> = values.iter().map(|value| value.symbol).collect();

    for operator in operators {
        if let Some(parent) = types.iter().find(|declaration| {
            declaration
                .constructors
                .iter()
                .any(|constructor| constructor.symbol == operator.symbol)
        }) {
            let complete = exported_types.iter().any(|exported| {
                exported.id == parent.id
                    && exported.constructors.as_ref().is_some_and(|constructors| {
                        parent
                            .constructors
                            .iter()
                            .all(|constructor| constructors.contains(&constructor.symbol))
                    })
            });
            if !complete {
                resolver.report(
                    ResolveErrorKind::TransitiveDctorExportError,
                    parent.name.clone(),
                    operator.span,
                );
            }
        } else if operator.target_name != operator.name
            && !values
                .iter()
                .any(|value| value.symbol == operator.symbol && value.name == operator.target_name)
        {
            resolver.report(
                ResolveErrorKind::TransitiveExportError,
                operator.target_name.clone(),
                operator.span,
            );
        }
    }
    for operator in type_operators {
        if operator.target_name != operator.name
            && !exported_types
                .iter()
                .any(|exported| exported.id == operator.id && exported.name == operator.target_name)
        {
            resolver.report(
                ResolveErrorKind::TransitiveExportError,
                operator.target_name.clone(),
                operator.span,
            );
        }
    }

    // A class member may only be exported together with its class.
    let mut member_owner: HashMap<SymbolId, (&hir::TypeDeclaration, String)> = HashMap::new();
    for declaration in types {
        if declaration.kind != TypeDeclarationKind::Class {
            continue;
        }
        for member in &declaration.members {
            member_owner.insert(member.symbol, (declaration, declaration.name.clone()));
        }
    }
    let mut reported_classes = HashSet::new();
    for value in values {
        if let Some((declaration, class_name)) = member_owner.get(&value.symbol)
            && !exported_type_ids.contains(&declaration.id)
            && reported_classes.insert(class_name.clone())
        {
            resolver.report(
                ResolveErrorKind::TransitiveExportError,
                class_name.clone(),
                value.span,
            );
        }
    }

    for exported in exported_types {
        let Some(declaration) = own.get(&exported.id).copied() else {
            continue;
        };
        let mut referenced = Vec::new();
        for constructor in &declaration.constructors {
            for field in &constructor.fields {
                collect_named_types(field, &mut referenced);
            }
        }
        if let Some(body) = &declaration.body {
            collect_named_types(body, &mut referenced);
        }
        for superclass in &declaration.superclasses {
            collect_named_types(superclass, &mut referenced);
        }
        for member in &declaration.members {
            if let Some(signature) = &member.signature {
                collect_named_types(signature, &mut referenced);
            }
        }

        let mut missing: Vec<String> = Vec::new();
        for id in referenced {
            if exported_type_ids.contains(&id) {
                continue;
            }
            if let Some(referenced) = own.get(&id)
                && !missing.contains(&referenced.name)
            {
                missing.push(referenced.name.clone());
            }
        }
        if declaration.kind == TypeDeclarationKind::Class {
            for member in &declaration.members {
                if !exported_symbols.contains(&member.symbol) && !missing.contains(&member.name) {
                    missing.push(member.name.clone());
                }
            }
        }
        for name in missing {
            resolver.report(
                ResolveErrorKind::TransitiveExportError,
                name,
                exported.name_span,
            );
        }
    }
}

fn collect_named_types(ty: &hir::Type, out: &mut Vec<TypeId>) {
    match &ty.kind {
        hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => out.push(*id),
        hir::TypeKind::Application(function, argument) => {
            collect_named_types(function, out);
            collect_named_types(argument, out);
        }
        hir::TypeKind::OperatorChain {
            operands,
            operators,
        } => {
            out.extend(operators.iter().map(|operator| operator.type_id));
            for operand in operands {
                collect_named_types(operand, out);
            }
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_named_types(parameter, out);
            collect_named_types(result, out);
        }
        hir::TypeKind::Forall { variables, body } => {
            for variable in variables {
                if let Some(kind) = &variable.kind {
                    collect_named_types(kind, out);
                }
            }
            collect_named_types(body, out);
        }
        hir::TypeKind::Constrained { constraint, body } => {
            collect_named_types(constraint, out);
            collect_named_types(body, out);
        }
        hir::TypeKind::Row { fields, tail } | hir::TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_named_types(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_named_types(tail, out);
            }
        }
        hir::TypeKind::Variable(_)
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
}
