use super::names::{Resolver, builtin_type, is_uppercase, split_qualified};
use super::{PlannedType, ResolveErrorKind};
use psrs_ast as ast;
use psrs_hir::{
    self as hir, ModuleId, Type as HirType, TypeDeclarationKind, TypeId, TypeKind as HirTypeKind,
};
use psrs_span::TextRange;

impl Resolver {
    pub(super) fn resolve_type(&mut self, expression: ast::Type) -> Option<HirType> {
        let span = expression.span;
        let kind = match expression.kind {
            ast::TypeKind::Name(name) => {
                if let Some(builtin) = builtin_type(&name.text) {
                    HirTypeKind::Constructor(builtin)
                } else if let Some((qualifier, member)) = split_qualified(&name.text) {
                    match self.lookup_qualified_type(&name.text, qualifier, member, name.span) {
                        Some(id) => HirTypeKind::Named(id),
                        None => return None,
                    }
                } else if is_uppercase(&name.text) {
                    match self.lookup_type_name(&name.text, name.span) {
                        Some(id) => HirTypeKind::Named(id),
                        None => return None,
                    }
                } else {
                    HirTypeKind::Variable(name.text)
                }
            }
            ast::TypeKind::Application(function, argument) => HirTypeKind::Application(
                Box::new(self.resolve_type(*function)?),
                Box::new(self.resolve_type(*argument)?),
            ),
            ast::TypeKind::Function { parameter, result } => HirTypeKind::Function {
                parameter: Box::new(self.resolve_type(*parameter)?),
                result: Box::new(self.resolve_type(*result)?),
            },
            ast::TypeKind::Forall { body, .. } => self.resolve_type(*body)?.kind,
        };
        Some(HirType { kind, span })
    }

    fn lookup_type_name(&mut self, text: &str, span: TextRange) -> Option<TypeId> {
        if let Some(id) = self.type_names.get(text) {
            return Some(*id);
        }
        if let Some(ids) = self.imported_types.get(text) {
            let first = ids[0];
            if ids.iter().all(|id| *id == first) {
                return Some(first);
            }
            self.report_conflict(text.to_string(), span);
            return None;
        }
        self.report(ResolveErrorKind::UnknownTypeName, text.to_string(), span);
        None
    }

    fn lookup_qualified_type(
        &mut self,
        text: &str,
        qualifier: &str,
        member: &str,
        span: TextRange,
    ) -> Option<TypeId> {
        let candidates = self.qualified_types.get(qualifier)?;
        let mut found: Option<(ModuleId, TypeId)> = None;
        let mut conflict = false;
        for candidate in candidates {
            let Some(id) = candidate.types.get(member) else {
                continue;
            };
            match found {
                None => found = Some((candidate.module, *id)),
                Some((module, existing)) if module != candidate.module || existing != *id => {
                    conflict = true;
                }
                _ => {}
            }
        }
        if conflict {
            self.report_conflict(text.to_string(), span);
            return None;
        }
        if let Some((_, id)) = found {
            return Some(id);
        }
        self.report(ResolveErrorKind::UnknownTypeName, text.to_string(), span);
        None
    }

    /// Resolves a planned type declaration into HIR. The plan holds the IDs and
    /// symbols allocated during conflict checking.
    pub(super) fn resolve_type_declaration(
        &mut self,
        plan: PlannedType,
        declaration: ast::TypeDeclaration,
    ) -> Option<hir::TypeDeclaration> {
        match declaration {
            ast::TypeDeclaration::Data(declaration) => {
                let constructors = declaration
                    .constructors
                    .into_iter()
                    .zip(plan.constructors)
                    .filter_map(|(constructor, symbol)| {
                        Some(hir::Constructor {
                            symbol,
                            name: constructor.name.text,
                            name_span: constructor.name.span,
                            fields: constructor
                                .fields
                                .into_iter()
                                .map(|field| self.resolve_type(field))
                                .collect::<Option<Vec<_>>>()?,
                            span: constructor.span,
                        })
                    })
                    .collect();
                Some(self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::Data,
                    declaration.parameters,
                    constructors,
                    Vec::new(),
                    None,
                    Vec::new(),
                    declaration.span,
                ))
            }
            ast::TypeDeclaration::Newtype(declaration) => {
                let constructors = declaration
                    .constructor
                    .into_iter()
                    .zip(plan.constructors)
                    .filter_map(|(constructor, symbol)| {
                        Some(hir::Constructor {
                            symbol,
                            name: constructor.name.text,
                            name_span: constructor.name.span,
                            fields: constructor
                                .fields
                                .into_iter()
                                .map(|field| self.resolve_type(field))
                                .collect::<Option<Vec<_>>>()?,
                            span: constructor.span,
                        })
                    })
                    .collect();
                Some(self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::Newtype,
                    declaration.parameters,
                    constructors,
                    Vec::new(),
                    None,
                    Vec::new(),
                    declaration.span,
                ))
            }
            ast::TypeDeclaration::TypeSynonym(declaration) => {
                let body = self.resolve_type(declaration.body)?;
                Some(self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::TypeSynonym,
                    declaration.parameters,
                    Vec::new(),
                    Vec::new(),
                    Some(body),
                    Vec::new(),
                    declaration.span,
                ))
            }
            ast::TypeDeclaration::Class(declaration) => {
                let mut superclasses = Vec::new();
                for superclass in declaration.superclasses {
                    superclasses.push(self.resolve_type(superclass)?);
                }
                let members = declaration
                    .members
                    .into_iter()
                    .zip(plan.members)
                    .filter_map(|(member, symbol)| {
                        let signature = match member.signature {
                            Some(signature) => Some(self.resolve_type(signature)?),
                            None => None,
                        };
                        Some(hir::ClassMember {
                            symbol,
                            name: member.name.text,
                            name_span: member.name.span,
                            signature,
                            span: member.span,
                        })
                    })
                    .collect();
                Some(self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::Class,
                    declaration.parameters,
                    Vec::new(),
                    members,
                    None,
                    superclasses,
                    declaration.span,
                ))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn type_declaration(
        &self,
        id: TypeId,
        name: ast::Name,
        kind: TypeDeclarationKind,
        parameters: Vec<ast::TypeParameter>,
        constructors: Vec<hir::Constructor>,
        members: Vec<hir::ClassMember>,
        body: Option<HirType>,
        superclasses: Vec<HirType>,
        span: TextRange,
    ) -> hir::TypeDeclaration {
        hir::TypeDeclaration {
            id,
            name: name.text,
            name_span: name.span,
            kind,
            parameters: parameters
                .into_iter()
                .map(|parameter| parameter.name.text)
                .collect(),
            constructors,
            members,
            body,
            superclasses,
            span,
        }
    }
}
