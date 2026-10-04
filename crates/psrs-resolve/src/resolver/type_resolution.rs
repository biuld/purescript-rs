use super::names::{
    Resolver, builtin_type, implicit_prim_class, is_uppercase, prim_type, split_qualified,
};
use super::{PlannedType, ResolveErrorKind};
use psrs_ast as ast;
use psrs_hir::{
    self as hir, ModuleId, Type as HirType, TypeDeclarationKind, TypeId, TypeKind as HirTypeKind,
    TypeReference,
};
use psrs_span::TextRange;

impl Resolver {
    pub(super) fn resolve_type(&mut self, expression: ast::Type) -> Option<HirType> {
        let span = expression.span;
        let kind = match expression.kind {
            ast::TypeKind::Wildcard => HirTypeKind::Wildcard,
            ast::TypeKind::Name(name) => {
                if let Some((qualifier, member)) = split_qualified(&name.text) {
                    let reference =
                        self.lookup_qualified_type(&name.text, qualifier, member, name.span)?;
                    self.type_reference_kind(reference)
                } else if is_uppercase(&name.text) {
                    let reference = self.lookup_type_name(&name.text, name.span)?;
                    self.type_reference_kind(reference)
                } else if matches!(name.text.as_str(), "->" | "~>") {
                    // Function arrows are type operators, not names imported
                    // from `Prim`. Keep their builtin identity independent of
                    // the source-level implicit import rules for `Function`.
                    HirTypeKind::Constructor(builtin_type(&name.text)?)
                } else {
                    HirTypeKind::Variable(name.text)
                }
            }
            ast::TypeKind::Application(function, argument) => HirTypeKind::Application(
                Box::new(self.resolve_type(*function)?),
                Box::new(self.resolve_type(*argument)?),
            ),
            ast::TypeKind::OperatorChain {
                operands,
                operators,
            } => HirTypeKind::OperatorChain {
                operands: operands
                    .into_iter()
                    .map(|operand| self.resolve_type(operand))
                    .collect::<Option<Vec<_>>>()?,
                operators: operators
                    .into_iter()
                    .map(|operator| self.resolve_type_operator(&operator.name.text, operator.span))
                    .collect::<Option<Vec<_>>>()?,
            },
            ast::TypeKind::Function { parameter, result } => HirTypeKind::Function {
                parameter: Box::new(self.resolve_type(*parameter)?),
                result: Box::new(self.resolve_type(*result)?),
            },
            ast::TypeKind::Forall { variables, body } => HirTypeKind::Forall {
                variables: variables
                    .into_iter()
                    .map(|variable| self.resolve_type_parameter(variable))
                    .collect::<Option<Vec<_>>>()?,
                body: Box::new(self.resolve_type(*body)?),
            },
            ast::TypeKind::Constrained { constraint, body } => HirTypeKind::Constrained {
                constraint: Box::new(self.resolve_type(*constraint)?),
                body: Box::new(self.resolve_type(*body)?),
            },
            ast::TypeKind::Row { fields, tail } => HirTypeKind::Row {
                fields: self.resolve_type_fields(fields)?,
                tail: match tail {
                    Some(tail) => Some(Box::new(self.resolve_type(*tail)?)),
                    None => None,
                },
            },
            ast::TypeKind::Record { fields, tail } => HirTypeKind::Record {
                fields: self.resolve_type_fields(fields)?,
                tail: match tail {
                    Some(tail) => Some(Box::new(self.resolve_type(*tail)?)),
                    None => None,
                },
            },
            ast::TypeKind::Integer(value) => HirTypeKind::Integer(value),
            ast::TypeKind::String(value) => HirTypeKind::String(value),
        };
        Some(HirType { kind, span })
    }

    fn resolve_type_operator(
        &mut self,
        name: &str,
        span: TextRange,
    ) -> Option<hir::ResolvedTypeOperator> {
        let reference = if let Some(fixity) = self.type_fixities.get(name) {
            match fixity.target {
                hir::FixityTarget::Type(reference) => reference,
                hir::FixityTarget::Value(_) => unreachable!("type fixities target types"),
            }
        } else if let Some((qualifier, member)) = split_qualified(name) {
            self.lookup_qualified_type(name, qualifier, member, span)?
        } else {
            self.lookup_type_name(name, span)?
        };
        let (associativity, precedence) = self
            .type_fixities
            .get(name)
            .map(|fixity| (fixity.associativity, fixity.precedence))
            .unwrap_or((hir::Associativity::Left, 9));
        Some(hir::ResolvedTypeOperator {
            head: self.resolved_type_head(reference),
            operator_span: span,
            associativity,
            precedence,
        })
    }

    fn resolve_type_parameter(
        &mut self,
        parameter: ast::TypeParameter,
    ) -> Option<hir::TypeParameter> {
        Some(hir::TypeParameter {
            name: parameter.name.text,
            name_span: parameter.name.span,
            kind: match parameter.kind {
                Some(kind) => Some(self.resolve_type(kind)?),
                None => None,
            },
        })
    }

    fn resolve_type_fields(&mut self, fields: Vec<ast::TypeField>) -> Option<Vec<hir::TypeField>> {
        fields
            .into_iter()
            .map(|field| {
                Some(hir::TypeField {
                    label: field.label.text,
                    label_span: field.label.span,
                    ty: self.resolve_type(field.ty)?,
                    span: field.span,
                })
            })
            .collect()
    }

    pub(super) fn lookup_type_name(
        &mut self,
        text: &str,
        span: TextRange,
    ) -> Option<TypeReference> {
        let local = self.type_names.get(text).copied().map(TypeReference::Named);
        let imported = self.imported_types.get(text);
        match (local, imported) {
            (Some(_), Some(_)) => {
                self.report_conflict(text.to_owned(), span);
                None
            }
            (Some(reference), None) => Some(reference),
            (None, Some(references)) => {
                let first = references[0];
                if references.iter().all(|reference| *reference == first) {
                    Some(first)
                } else {
                    self.report_conflict(text.to_owned(), span);
                    None
                }
            }
            (None, None)
                if self
                    .imports
                    .iter()
                    .any(|import| import.module_name == "Prim") =>
            {
                self.report(ResolveErrorKind::UnknownTypeName, text.to_owned(), span);
                None
            }
            (None, None) => match builtin_type(text)
                .map(TypeReference::Builtin)
                .or_else(|| implicit_prim_class(text))
            {
                Some(reference) => Some(reference),
                None => {
                    self.report(ResolveErrorKind::UnknownTypeName, text.to_owned(), span);
                    None
                }
            },
        }
    }

    pub(super) fn lookup_qualified_type(
        &mut self,
        text: &str,
        qualifier: &str,
        member: &str,
        span: TextRange,
    ) -> Option<TypeReference> {
        let has_explicit_prim_qualifier = self
            .imports
            .iter()
            .any(|import| import.module_name == "Prim" && import.alias.as_deref() == Some("Prim"));
        let default_prim = if qualifier == "Prim" && !has_explicit_prim_qualifier {
            prim_type(member)
                .map(|builtin| (ModuleId::COMPILER_PRELUDE, TypeReference::Builtin(builtin)))
        } else {
            None
        };
        let candidates = self.qualified_types.get(qualifier);
        if candidates.is_none() && default_prim.is_none() {
            self.report(ResolveErrorKind::UnknownTypeName, text.to_owned(), span);
            return None;
        }
        let mut found: Option<(ModuleId, TypeReference)> = default_prim;
        let mut conflict = false;
        for candidate in candidates.into_iter().flatten() {
            let Some(reference) = candidate.types.get(member) else {
                continue;
            };
            match found {
                None => found = Some((candidate.module, *reference)),
                Some((module, existing))
                    if module != candidate.module || existing != *reference =>
                {
                    conflict = true;
                }
                _ => {}
            }
        }
        if conflict {
            self.report_conflict(text.to_owned(), span);
            return None;
        }
        if let Some((_, reference)) = found {
            return Some(reference);
        }
        self.report(ResolveErrorKind::UnknownTypeName, text.to_owned(), span);
        None
    }

    fn resolved_type_head(&self, reference: TypeReference) -> hir::ResolvedTypeHead {
        match reference {
            TypeReference::Builtin(builtin) => hir::ResolvedTypeHead::Builtin(builtin),
            TypeReference::Named(id) if self.opaque_types.contains(&id) => {
                hir::ResolvedTypeHead::Opaque(id)
            }
            TypeReference::Named(id) => hir::ResolvedTypeHead::Named(id),
        }
    }

    fn type_reference_kind(&self, reference: TypeReference) -> HirTypeKind {
        match self.resolved_type_head(reference) {
            hir::ResolvedTypeHead::Builtin(builtin) => HirTypeKind::Constructor(builtin),
            hir::ResolvedTypeHead::Named(id) => HirTypeKind::Named(id),
            hir::ResolvedTypeHead::Opaque(id) => HirTypeKind::Opaque(id),
        }
    }

    /// Records opacity of imported foreign data so signatures in this module
    /// mention `Opaque` without looking at the declaring module again.
    pub(super) fn note_imported_opaque_types(&mut self) {
        let imported = self
            .imports
            .iter()
            .flat_map(|import| import.types.iter())
            .filter(|imported| imported.opaque)
            .filter_map(|imported| match imported.reference {
                TypeReference::Named(id) => Some(id),
                TypeReference::Builtin(_) => None,
            })
            .collect::<Vec<_>>();
        self.opaque_types.extend(imported);
    }

    /// Resolves a planned type declaration into HIR. The plan holds the IDs and
    /// symbols allocated during conflict checking.
    pub(super) fn resolve_type_declaration(
        &mut self,
        plan: PlannedType,
        declaration: ast::TypeDeclaration,
        role_declaration: Option<ast::RoleDeclaration>,
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
                self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::Data,
                    declaration.parameters,
                    constructors,
                    Vec::new(),
                    None,
                    Vec::new(),
                    Vec::new(),
                    declaration.kind_signature,
                    lower_role_declaration(role_declaration),
                    declaration.span,
                )
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
                self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::Newtype,
                    declaration.parameters,
                    constructors,
                    Vec::new(),
                    None,
                    Vec::new(),
                    Vec::new(),
                    declaration.kind_signature,
                    lower_role_declaration(role_declaration),
                    declaration.span,
                )
            }
            ast::TypeDeclaration::TypeSynonym(declaration) => {
                let body = self.resolve_type(declaration.body)?;
                self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::TypeSynonym,
                    declaration.parameters,
                    Vec::new(),
                    Vec::new(),
                    Some(body),
                    Vec::new(),
                    Vec::new(),
                    declaration.kind_signature,
                    None,
                    declaration.span,
                )
            }
            ast::TypeDeclaration::Foreign(declaration) => self.type_declaration(
                plan.id,
                declaration.name,
                TypeDeclarationKind::Foreign,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                Vec::new(),
                Vec::new(),
                Some(declaration.declared_kind),
                lower_role_declaration(role_declaration),
                declaration.span,
            ),
            ast::TypeDeclaration::Class(declaration) => {
                let mut superclasses = Vec::new();
                for superclass in declaration.superclasses {
                    superclasses.push(self.resolve_type(superclass)?);
                }
                let fundeps = declaration
                    .fundeps
                    .into_iter()
                    .map(|fundep| hir::FunctionalDependency {
                        from: fundep.from.into_iter().map(|name| name.text).collect(),
                        to: fundep.to.into_iter().map(|name| name.text).collect(),
                        span: fundep.span,
                    })
                    .collect();
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
                self.type_declaration(
                    plan.id,
                    declaration.name,
                    TypeDeclarationKind::Class,
                    declaration.parameters,
                    Vec::new(),
                    members,
                    None,
                    superclasses,
                    fundeps,
                    declaration.kind_signature,
                    None,
                    declaration.span,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn type_declaration(
        &mut self,
        id: TypeId,
        name: ast::Name,
        kind: TypeDeclarationKind,
        parameters: Vec<ast::TypeParameter>,
        constructors: Vec<hir::Constructor>,
        members: Vec<hir::ClassMember>,
        body: Option<HirType>,
        superclasses: Vec<HirType>,
        fundeps: Vec<hir::FunctionalDependency>,
        kind_signature: Option<ast::Type>,
        declared_roles: Option<hir::RoleDeclaration>,
        span: TextRange,
    ) -> Option<hir::TypeDeclaration> {
        let parameters = parameters
            .into_iter()
            .map(|parameter| self.resolve_type_parameter(parameter))
            .collect::<Option<Vec<_>>>()?;
        let declared_kind = match kind_signature {
            Some(kind) => Some(self.resolve_type(kind)?),
            None => None,
        };
        Some(hir::TypeDeclaration {
            id,
            name: name.text,
            name_span: name.span,
            kind,
            parameters,
            constructors,
            members,
            body,
            superclasses,
            fundeps,
            declared_kind,
            declared_roles,
            span,
        })
    }
}

fn lower_role_declaration(
    declaration: Option<ast::RoleDeclaration>,
) -> Option<hir::RoleDeclaration> {
    declaration.map(|declaration| hir::RoleDeclaration {
        roles: declaration
            .roles
            .into_iter()
            .map(|annotation| {
                let role = match annotation.role {
                    ast::TypeRole::Nominal => hir::Role::Nominal,
                    ast::TypeRole::Representational => hir::Role::Representational,
                    ast::TypeRole::Phantom => hir::Role::Phantom,
                };
                (role, annotation.span)
            })
            .collect(),
        span: declaration.span,
    })
}
