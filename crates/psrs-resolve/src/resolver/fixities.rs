use super::ResolveErrorKind;
use super::names::{Resolver, split_qualified};
use psrs_ast as ast;
use psrs_hir as hir;

impl Resolver {
    /// Resolves each fixity target through the same value or type namespace
    /// used by ordinary references, then registers its operator alias.
    pub(super) fn resolve_fixity_declarations(
        &mut self,
        declarations: &[ast::FixityDeclaration],
    ) -> Vec<hir::Fixity> {
        let mut local_fixities = Vec::with_capacity(declarations.len());
        for declaration in declarations {
            match declaration.namespace {
                ast::FixityNamespace::Value => {
                    let Some(target) =
                        self.lookup_global(&declaration.target.text, declaration.target.span)
                    else {
                        continue;
                    };
                    if declaration.operator.text != declaration.target.text
                        && self
                            .globals
                            .insert(declaration.operator.text.clone(), target)
                            .is_some()
                    {
                        self.report(
                            ResolveErrorKind::DuplicateDeclaration,
                            declaration.operator.text.clone(),
                            declaration.operator.span,
                        );
                    }
                    local_fixities.push(hir::Fixity {
                        namespace: hir::FixityNamespace::Value,
                        operator: declaration.operator.text.clone(),
                        target_name: declaration.target.text.clone(),
                        target: hir::FixityTarget::Value(target),
                        associativity: lower_associativity(declaration.associativity),
                        precedence: declaration.precedence,
                        span: declaration.span,
                    });
                }
                ast::FixityNamespace::Type => {
                    let target_name = &declaration.target.text;
                    let target = if let Some((qualifier, member)) = split_qualified(target_name) {
                        self.lookup_qualified_type(
                            target_name,
                            qualifier,
                            member,
                            declaration.target.span,
                        )
                    } else {
                        self.lookup_type_name(target_name, declaration.target.span)
                    };
                    let Some(target) = target else {
                        continue;
                    };
                    if declaration.operator.text != declaration.target.text
                        && self.type_names.contains_key(&declaration.operator.text)
                    {
                        self.report(
                            ResolveErrorKind::DeclConflict,
                            declaration.operator.text.clone(),
                            declaration.operator.span,
                        );
                    }
                    local_fixities.push(hir::Fixity {
                        namespace: hir::FixityNamespace::Type,
                        operator: declaration.operator.text.clone(),
                        target_name: declaration.target.text.clone(),
                        target: hir::FixityTarget::Type(target),
                        associativity: lower_associativity(declaration.associativity),
                        precedence: declaration.precedence,
                        span: declaration.span,
                    });
                }
            }
        }

        let (value_fixities, type_fixities) =
            super::operators::merge_fixities(local_fixities.clone(), &self.imports);
        self.fixities = value_fixities;
        self.type_fixities = type_fixities;
        local_fixities
    }
}

fn lower_associativity(associativity: ast::Associativity) -> hir::Associativity {
    match associativity {
        ast::Associativity::Left => hir::Associativity::Left,
        ast::Associativity::Right => hir::Associativity::Right,
        ast::Associativity::None => hir::Associativity::None,
    }
}
