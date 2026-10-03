use super::super::{ResolveErrorKind, names::Resolver};
use psrs_hir::{
    self as hir, ExportedOperator, ExportedSymbol, ExportedType, ExportedTypeOperator, SymbolId,
    TypeDeclarationKind, TypeId, TypeReference,
};
use std::collections::{HashMap, HashSet};

impl Resolver {
    /// Reports in-module declarations required by exported types or values.
    pub(super) fn check_transitive_exports(
        &mut self,
        types: &[hir::TypeDeclaration],
        declarations: &[hir::Declaration],
        exported_types: &[ExportedType],
        values: &[ExportedSymbol],
        operators: &[ExportedOperator],
        type_operators: &[ExportedTypeOperator],
    ) {
        let own: HashMap<TypeId, &hir::TypeDeclaration> = types
            .iter()
            .map(|declaration| (declaration.id, declaration))
            .collect();
        let exported_type_references: HashSet<TypeReference> = exported_types
            .iter()
            .map(|exported| exported.reference)
            .collect();
        let exported_symbols: HashSet<SymbolId> = values.iter().map(|value| value.symbol).collect();
        let own_value_symbols: HashSet<SymbolId> = declarations
            .iter()
            .map(|declaration| declaration.symbol)
            .collect();

        // An exported constructor alias must expose its complete parent type.
        for operator in operators {
            if let Some(parent) = types.iter().find(|declaration| {
                declaration
                    .constructors
                    .iter()
                    .any(|constructor| constructor.symbol == operator.symbol)
            }) {
                let complete = exported_types.iter().any(|exported| {
                    exported.reference == TypeReference::Named(parent.id)
                        && exported.constructors.as_ref().is_some_and(|constructors| {
                            parent
                                .constructors
                                .iter()
                                .all(|constructor| constructors.contains(&constructor.symbol))
                        })
                });
                if !complete {
                    self.report(
                        ResolveErrorKind::TransitiveDctorExportError,
                        parent.name.clone(),
                        operator.span,
                    );
                }
            } else if operator.target_name != operator.name
                && own_value_symbols.contains(&operator.symbol)
                && !values.iter().any(|value| {
                    value.symbol == operator.symbol && value.name == operator.target_name
                })
            {
                self.report(
                    ResolveErrorKind::TransitiveExportError,
                    operator.target_name.clone(),
                    operator.span,
                );
            }
        }
        for operator in type_operators {
            let TypeReference::Named(id) = operator.reference else {
                continue;
            };
            if operator.target_name != operator.name
                && own.contains_key(&id)
                && !exported_types.iter().any(|exported| {
                    exported.reference == operator.reference
                        && exported.name == operator.target_name
                })
            {
                self.report(
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
                && !exported_type_references.contains(&TypeReference::Named(declaration.id))
                && reported_classes.insert(class_name.clone())
            {
                self.report(
                    ResolveErrorKind::TransitiveExportError,
                    class_name.clone(),
                    value.span,
                );
            }
        }

        for exported in exported_types {
            let TypeReference::Named(exported_id) = exported.reference else {
                continue;
            };
            let Some(declaration) = own.get(&exported_id).copied() else {
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
            for parameter in &declaration.parameters {
                if let Some(kind) = &parameter.kind {
                    collect_named_types(kind, &mut referenced);
                }
            }
            if let Some(kind) = &declaration.declared_kind {
                collect_named_types(kind, &mut referenced);
            }

            let mut missing: Vec<String> = Vec::new();
            for id in referenced {
                if exported_type_references.contains(&TypeReference::Named(id)) {
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
                    if !exported_symbols.contains(&member.symbol) && !missing.contains(&member.name)
                    {
                        missing.push(member.name.clone());
                    }
                }
            }
            for name in missing {
                self.report(
                    ResolveErrorKind::TransitiveExportError,
                    name,
                    exported.name_span,
                );
            }
        }

        let constructor_owners: HashMap<SymbolId, TypeId> = types
            .iter()
            .flat_map(|declaration| {
                declaration
                    .constructors
                    .iter()
                    .map(move |constructor| (constructor.symbol, declaration.id))
            })
            .collect();
        let own_declarations: HashMap<SymbolId, &hir::Declaration> = declarations
            .iter()
            .map(|declaration| (declaration.symbol, declaration))
            .collect();
        for value in values {
            let mut required = Vec::new();
            if let Some(owner) = constructor_owners.get(&value.symbol) {
                required.push(*owner);
            }
            if let Some(declaration) = own_declarations.get(&value.symbol)
                && let Some(signature) = &declaration.signature
            {
                collect_named_types(signature, &mut required);
            }
            // Explicit signatures are checked at P3, where their named HIR
            // references are already resolved. Inferred public value types are
            // checked after inference at P5, where the complete type has stable
            // TypeIds. Do not guess them from expression syntax here.
            required.sort_by_key(|id| (id.module.0, id.index));
            required.dedup();
            for id in required {
                if exported_type_references.contains(&TypeReference::Named(id)) {
                    continue;
                }
                if let Some(hidden) = own.get(&id) {
                    self.report(
                        ResolveErrorKind::TransitiveExportError,
                        hidden.name.clone(),
                        value.span,
                    );
                }
            }
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
            out.extend(operators.iter().filter_map(|operator| match operator.head {
                hir::ResolvedTypeHead::Builtin(_) => None,
                hir::ResolvedTypeHead::Named(id) | hir::ResolvedTypeHead::Opaque(id) => Some(id),
            }));
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
        hir::TypeKind::Wildcard
        | hir::TypeKind::Variable(_)
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
}
