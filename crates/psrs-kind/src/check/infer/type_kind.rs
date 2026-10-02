//! Kind inference for a source type: the kind a type denotes, given the
//! kinds bound by its binders.

use super::*;
use crate::kind::{primitive_kind, type_kind};
use psrs_hir::BuiltinType;

impl Checker<'_> {
    // ----- Kind inference ----------------------------------------------

    pub(super) fn kind_of_type(
        &mut self,
        ty: &hir::Type,
        scope: &mut HashMap<String, Kind>,
    ) -> Kind {
        self.kind_of_type_against(ty, scope, true)
    }

    /// The kind a type denotes.
    ///
    /// `report_partial_synonym` is off where the type is elaborated against a
    /// kind it is required to have rather than read on its own. An instance head
    /// is the case that matters: each of its arguments is checked against the
    /// parameter kind its class declares, and `(->)` applied to one argument is
    /// how a higher-kinded class parameter is written — `Functor ((->) r)` is
    /// the standard library's own instance, which `purs` accepts. Read on its
    /// own, that same argument is the function arrow partway applied, which is
    /// the `PartiallyAppliedSynonym` of `failing/TypeSynonyms9.purs`.
    pub(super) fn kind_of_type_against(
        &mut self,
        ty: &hir::Type,
        scope: &mut HashMap<String, Kind>,
        report_partial_synonym: bool,
    ) -> Kind {
        match &ty.kind {
            TypeKind::Wildcard => self.fresh(),
            TypeKind::Application(..) => {
                let (head, arguments) = flatten_spine(ty);
                if report_partial_synonym {
                    self.check_partial_synonym(head, arguments.len(), ty.span);
                }
                let mut function = self.head_kind(head, scope);
                for argument in arguments {
                    let argument =
                        self.kind_of_type_against(argument, scope, report_partial_synonym);
                    let result = self.fresh();
                    self.unify(
                        function,
                        Kind::Function(Box::new(argument), Box::new(result.clone())),
                        ty.span,
                    );
                    function = result;
                }
                function
            }
            TypeKind::OperatorChain { .. } => {
                self.report(
                    UNLOWERED_TYPE_OPERATOR,
                    ty.span,
                    "type operator chain reached kind checking before P4",
                );
                self.fresh()
            }
            TypeKind::Named(id) | TypeKind::Opaque(id) => {
                self.check_partial_synonym(ty, 0, ty.span);
                self.instantiate_named(*id, ty.span)
            }
            TypeKind::Constructor(BuiltinType::Function) => {
                // A bare `(->)` is the function type constructor itself, of kind
                // `Type -> Type -> Type`, and applies nothing. It is how a
                // higher-kinded class parameter is written — `Functor ((->) r)`
                // is the standard library's own instance — so it is not a
                // partially applied synonym however it is spelled. Only an
                // actual application reaches the arity rule, through the spine
                // walk above.
                primitive_kind(BuiltinType::Function)
            }
            _ => self.kind_of_atom(ty, scope),
        }
    }

    pub(super) fn head_kind(
        &mut self,
        head: &hir::Type,
        scope: &mut HashMap<String, Kind>,
    ) -> Kind {
        match &head.kind {
            TypeKind::Named(id) | TypeKind::Opaque(id) => self.instantiate_named(*id, head.span),
            // The one primitive kind table answers what kind a primitive has
            // when it is used as a type.
            TypeKind::Constructor(builtin) => primitive_kind(*builtin),
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| self.fresh()),
            _ => self.kind_of_atom(head, scope),
        }
    }

    pub(super) fn kind_of_atom(
        &mut self,
        ty: &hir::Type,
        scope: &mut HashMap<String, Kind>,
    ) -> Kind {
        match &ty.kind {
            TypeKind::Wildcard => self.fresh(),
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| {
                let kind = self.fresh();
                scope.insert(name.clone(), kind.clone());
                kind
            }),
            TypeKind::Constructor(builtin) => primitive_kind(*builtin),
            TypeKind::Named(id) | TypeKind::Opaque(id) => self.instantiate_named(*id, ty.span),
            TypeKind::Application(..) => self.kind_of_type(ty, scope),
            TypeKind::OperatorChain { .. } => {
                self.report(
                    UNLOWERED_TYPE_OPERATOR,
                    ty.span,
                    "type operator chain reached kind checking before P4",
                );
                self.fresh()
            }
            TypeKind::Function { parameter, result } => {
                let parameter_kind = self.kind_of_type(parameter, scope);
                self.unify(parameter_kind, type_kind(), parameter.span);
                let result_kind = self.kind_of_type(result, scope);
                self.unify(result_kind, type_kind(), result.span);
                type_kind()
            }
            TypeKind::Forall { variables, body } => {
                let saved = scope.clone();
                for variable in variables {
                    let kind = match &variable.kind {
                        Some(annotation) => self.denote_annotation(annotation, scope),
                        None => self.fresh(),
                    };
                    scope.insert(variable.name.clone(), kind);
                }
                let kind = self.kind_of_type(body, scope);
                *scope = saved;
                kind
            }
            TypeKind::Constrained { constraint, body } => {
                let constraint_kind = self.kind_of_type(constraint, scope);
                self.unify(constraint_kind, super::constraint_kind(), constraint.span);
                self.kind_of_type(body, scope)
            }
            TypeKind::Row { fields, tail } => {
                let field_kind = self.fresh();
                for field in fields {
                    let kind = self.kind_of_type(&field.ty, scope);
                    self.unify(kind, field_kind.clone(), field.ty.span);
                }
                if let Some(tail) = tail {
                    let tail_kind = self.kind_of_type(tail, scope);
                    self.unify(tail_kind, Kind::row(field_kind.clone()), tail.span);
                }
                Kind::row(field_kind)
            }
            TypeKind::Record { fields, tail } => {
                let field_kind = self.fresh();
                for field in fields {
                    let kind = self.kind_of_type(&field.ty, scope);
                    self.unify(kind, field_kind.clone(), field.ty.span);
                }
                if let Some(tail) = tail {
                    let tail_kind = self.kind_of_type(tail, scope);
                    self.unify(tail_kind, Kind::row(field_kind), tail.span);
                }
                type_kind()
            }
            TypeKind::Integer(_) => Kind::Builtin(BuiltinType::Int),
            TypeKind::String(_) => Kind::Builtin(BuiltinType::Symbol),
        }
    }

    /// Rejects an unsaturated synonym read in a kind annotation.
    ///
    /// Saturation is a rule about how a synonym is *used*, so it is checked on
    /// the expression rather than inside [`denote_kind`](crate::denote_kind):
    /// `newtype C (a :: (->) (Type -> Type) -> Type)` is a partially applied
    /// synonym whether the expression is read as a kind or as a type.
    pub(in crate::check) fn check_annotation_saturation(&mut self, ty: &hir::Type) {
        match &ty.kind {
            TypeKind::Application(..) => {
                let (head, arguments) = flatten_spine(ty);
                self.check_partial_synonym(head, arguments.len(), ty.span);
                self.check_annotation_saturation(head);
                for argument in arguments {
                    self.check_annotation_saturation(argument);
                }
            }
            TypeKind::Function { parameter, result } => {
                self.check_annotation_saturation(parameter);
                self.check_annotation_saturation(result);
            }
            TypeKind::Forall { variables, body } => {
                for variable in variables {
                    if let Some(annotation) = &variable.kind {
                        self.check_annotation_saturation(annotation);
                    }
                }
                self.check_annotation_saturation(body);
            }
            TypeKind::Constrained { constraint, body } => {
                self.check_annotation_saturation(constraint);
                self.check_annotation_saturation(body);
            }
            TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
                for field in fields {
                    self.check_annotation_saturation(&field.ty);
                }
                if let Some(tail) = tail {
                    self.check_annotation_saturation(tail);
                }
            }
            TypeKind::Variable(_)
            | TypeKind::Wildcard
            | TypeKind::Constructor(_)
            | TypeKind::Named(_)
            | TypeKind::Opaque(_)
            | TypeKind::OperatorChain { .. }
            | TypeKind::Integer(_)
            | TypeKind::String(_) => {}
        }
    }

    pub(super) fn check_partial_synonym(
        &mut self,
        head: &hir::Type,
        arguments: usize,
        span: TextRange,
    ) {
        match &head.kind {
            TypeKind::Named(id) => {
                if let Some(arity) = self.synonym_arity.get(id)
                    && arguments < *arity
                {
                    self.report(
                        PARTIALLY_APPLIED_SYNONYM,
                        span,
                        "a type synonym must be fully applied",
                    );
                }
            }
            TypeKind::Constructor(BuiltinType::Function) if arguments < 2 => {
                self.report(
                    PARTIALLY_APPLIED_SYNONYM,
                    span,
                    "a type synonym must be fully applied",
                );
            }
            _ => {}
        }
    }
}
