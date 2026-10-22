//! Kind inference for a source type: the kind a type denotes, given the
//! kinds bound by its binders.

use super::*;
use crate::kind::{primitive_kind, type_kind};
use crate::solve::substitute;
use psrs_hir::BuiltinType;
use std::collections::HashMap;

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
    /// the standard library's own instance, which `purs` accepts. The function
    /// arrow is a builtin type constructor, not a synonym, so a partial
    /// application of it is never a `PartiallyAppliedSynonym`.
    pub(super) fn kind_of_type_against(
        &mut self,
        ty: &hir::Type,
        scope: &mut HashMap<String, Kind>,
        report_partial_synonym: bool,
    ) -> Kind {
        if report_partial_synonym {
            self.check_annotation_saturation(ty);
        }
        self.infer_type_kind(ty, scope)
    }

    // Infer the original application so even arguments discarded by synonym
    // expansion must satisfy the declaration's checked parameter kinds.
    fn infer_type_kind(&mut self, ty: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        match &ty.kind {
            TypeKind::Wildcard | TypeKind::Hole(_) => self.fresh(),
            TypeKind::Application(..) => {
                let (head, arguments) = flatten_spine(ty);
                let head_kind = self.head_kind(head, scope);
                let mut function = self.instantiate_quantified_kind(head_kind);
                for argument in arguments {
                    let argument_kind = self.infer_type_kind(argument, scope);
                    function = self.apply_kind(function, argument_kind, ty.span);
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
            TypeKind::Named(id) | TypeKind::Opaque(id) => self.instantiate_named(*id, ty.span),
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
            TypeKind::Wildcard | TypeKind::Hole(_) => self.fresh(),
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| {
                let kind = self.fresh();
                scope.insert(name.clone(), kind.clone());
                kind
            }),
            TypeKind::Constructor(builtin) => primitive_kind(*builtin),
            TypeKind::Named(id) | TypeKind::Opaque(id) => self.instantiate_named(*id, ty.span),
            TypeKind::Application(..) => self.infer_type_kind(ty, scope),
            TypeKind::OperatorChain { .. } => {
                self.report(
                    UNLOWERED_TYPE_OPERATOR,
                    ty.span,
                    "type operator chain reached kind checking before P4",
                );
                self.fresh()
            }
            TypeKind::Function { parameter, result } => {
                let parameter_kind = self.infer_type_kind(parameter, scope);
                self.unify(parameter_kind, type_kind(), parameter.span);
                let result_kind = self.infer_type_kind(result, scope);
                self.unify(result_kind, type_kind(), result.span);
                type_kind()
            }
            TypeKind::Forall { variables, body } => {
                let saved = scope.clone();
                for variable in variables {
                    let kind = match &variable.kind {
                        Some(annotation) => self.denote_annotation(annotation, scope),
                        None => self.fresh_closed_unknown(),
                    };
                    scope.insert(variable.name.clone(), kind);
                }
                let kind = self.infer_type_kind(body, scope);
                *scope = saved;
                kind
            }
            TypeKind::Constrained { constraint, body } => {
                let constraint_kind = self.infer_type_kind(constraint, scope);
                self.unify(constraint_kind, super::constraint_kind(), constraint.span);
                self.infer_type_kind(body, scope)
            }
            TypeKind::Row { fields, tail } => self.infer_row_kind(fields, tail.as_deref(), scope),
            TypeKind::Record { fields, tail } => {
                let row_kind = self.infer_row_kind(fields, tail.as_deref(), scope);
                self.apply_kind(primitive_kind(BuiltinType::Record), row_kind, ty.span)
            }
            TypeKind::Integer(_) => Kind::Builtin(BuiltinType::Int),
            TypeKind::String(_) => Kind::Builtin(BuiltinType::Symbol),
            TypeKind::Kinded { expression, kind } => {
                // Official `inferKind` for `KindedType`: the annotation is the
                // kind, and the inner type is checked against it. The
                // annotation is not reduced to the kind that annotation has.
                let expected = match crate::denote::denote_kind_annotation(kind, &mut self.state) {
                    Some(kind) => kind,
                    None => {
                        self.report(
                            UNLOWERED_TYPE_OPERATOR,
                            kind.span,
                            "type operator chain reached kind checking before P4",
                        );
                        self.fresh()
                    }
                };
                let inferred = self.infer_type_kind(expression, scope);
                self.unify(inferred, expected.clone(), ty.span);
                self.state.resolve(expected)
            }
        }
    }

    fn apply_kind(&mut self, function: Kind, argument: Kind, span: TextRange) -> Kind {
        self.report_escaped_kind_skolem(span, &function, &argument);
        let result = self.fresh();
        self.unify(
            function,
            Kind::Function(Box::new(argument), Box::new(result.clone())),
            span,
        );
        self.instantiate_quantified_kind(result)
    }

    fn infer_row_kind(
        &mut self,
        fields: &[hir::TypeField],
        tail: Option<&hir::Type>,
        scope: &mut HashMap<String, Kind>,
    ) -> Kind {
        let field_kind = self.fresh();
        for field in fields {
            let kind = self.infer_type_kind(&field.ty, scope);
            self.unify(kind, field_kind.clone(), field.ty.span);
        }
        if let Some(tail) = tail {
            let tail_kind = self.infer_type_kind(tail, scope);
            self.unify(tail_kind, Kind::row(field_kind.clone()), tail.span);
        }
        Kind::row(field_kind)
    }

    /// Rejects an unsaturated synonym read in a kind annotation.
    ///
    /// Saturation is a rule about how a synonym is *used*, so it is checked on
    /// the expression rather than inside [`denote_kind`](crate::denote_kind):
    /// `newtype C (a :: (->) (Type -> Type) -> Type)` is a partially applied
    /// synonym whether the expression is read as a kind or as a type.
    pub(in crate::check) fn check_annotation_saturation(&mut self, ty: &hir::Type) {
        // A saturated outer alias can apply or discard a partial alias
        // argument. Only aliases surviving substitution require saturation.
        let expanded = super::super::synonyms::expand(ty, &self.synonyms);
        self.check_normalized_saturation(expanded.as_ref().unwrap_or(ty), ty.span);
    }

    fn check_normalized_saturation(&mut self, ty: &hir::Type, use_span: TextRange) {
        match &ty.kind {
            TypeKind::Application(..) => {
                let (head, arguments) = flatten_spine(ty);
                self.check_partial_synonym(head, arguments.len(), use_span);
                for argument in arguments {
                    self.check_normalized_saturation(argument, use_span);
                }
            }
            TypeKind::Function { parameter, result } => {
                self.check_normalized_saturation(parameter, use_span);
                self.check_normalized_saturation(result, use_span);
            }
            TypeKind::Forall { variables, body } => {
                for variable in variables {
                    if let Some(annotation) = &variable.kind {
                        self.check_normalized_saturation(annotation, use_span);
                    }
                }
                self.check_normalized_saturation(body, use_span);
            }
            TypeKind::Constrained { constraint, body } => {
                self.check_normalized_saturation(constraint, use_span);
                self.check_normalized_saturation(body, use_span);
            }
            TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
                for field in fields {
                    self.check_normalized_saturation(&field.ty, use_span);
                }
                if let Some(tail) = tail {
                    self.check_normalized_saturation(tail, use_span);
                }
            }
            TypeKind::Kinded { expression, kind } => {
                self.check_normalized_saturation(expression, use_span);
                self.check_normalized_saturation(kind, use_span);
            }
            TypeKind::Named(_) | TypeKind::Opaque(_) => {
                self.check_partial_synonym(ty, 0, use_span);
            }
            TypeKind::Variable(_)
            | TypeKind::Wildcard
            | TypeKind::Hole(_)
            | TypeKind::Constructor(_)
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
        if let TypeKind::Named(id) = &head.kind
            && let Some(synonym) = self.synonyms.get(id)
            && arguments < synonym.parameters.len()
        {
            self.report(
                PARTIALLY_APPLIED_SYNONYM,
                span,
                "a type synonym must be fully applied",
            );
        }
    }

    /// Instantiates a leading quantified kind with a fresh kind variable.
    ///
    /// This is the `inferAppKind` arm for a `ForAll` function kind: the
    /// quantifier is a kind parameter of the constructor being applied, so it
    /// is replaced, not skolemized. A `forall` nested under an arrow stays
    /// where it is.
    fn instantiate_quantified_kind(&mut self, mut kind: Kind) -> Kind {
        loop {
            kind = self.state.resolve(kind);
            let Kind::ForAll { variable, body } = &kind else {
                return kind;
            };
            let fresh = self.fresh();
            kind = substitute(body, &HashMap::from([(*variable, fresh)]));
        }
    }

    /// Reports `EscapedSkolem` when a synonym body captures a quantified kind.
    ///
    /// Official `checkEscapedSkolems` walks the elaborated synonym and rejects
    /// a skolem left behind by subsuming a polymorphic kind into an inferred
    /// parameter. That happens when the argument's kind still has a `forall`
    /// in a negative position after the argument's own leading quantifiers
    /// would have been instantiated, and the parameter waiting for it is a
    /// flexible unknown (`type B = Proxy A`).
    fn report_escaped_kind_skolem(&mut self, span: TextRange, function: &Kind, argument: &Kind) {
        if !self.in_synonym {
            return;
        }
        let function = self.state.resolve(function.clone());
        let Kind::Function(parameter, _) = function else {
            return;
        };
        let Kind::Variable(parameter) = self.state.resolve(*parameter) else {
            return;
        };
        if self.state.is_rigid(parameter) {
            return;
        }
        if forall_in_negative_position(&self.state, argument) {
            self.report(
                ESCAPED_SKOLEM,
                span,
                "a quantified kind escapes its scope in this type synonym",
            );
        }
    }
}

/// Whether `kind` has a `forall` in a negative position.
///
/// A leading quantifier is instantiated, so it does not count. A quantifier
/// under the domain of an arrow is what subsumption skolemizes.
fn forall_in_negative_position(state: &crate::solve::KindState, kind: &Kind) -> bool {
    fn go(state: &crate::solve::KindState, kind: &Kind, negative: bool) -> bool {
        match state.resolve(kind.clone()) {
            Kind::ForAll { .. } if negative => true,
            Kind::ForAll { body, .. } => go(state, &body, false),
            Kind::Function(parameter, result) => {
                go(state, &parameter, !negative) || go(state, &result, negative)
            }
            Kind::App(function, argument) => {
                go(state, &function, negative) || go(state, &argument, negative)
            }
            Kind::RowExtend { field, tail, .. } => {
                go(state, &field, negative) || go(state, &tail, negative)
            }
            Kind::Variable(_)
            | Kind::Builtin(_)
            | Kind::Named(_)
            | Kind::RowEmpty
            | Kind::TypeVariable(_) => false,
        }
    }
    go(state, kind, false)
}
