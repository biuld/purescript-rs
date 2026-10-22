//! Well-scoped quantification of a declared kind.
//!
//! Official `checkKindDeclaration` (`TypeChecker/Kinds.hs`) infers the
//! signature, generalizes leftover unknowns in front of the explicit
//! `forall` binders (`generalizeUnknowns`), then `checkQuantification` rejects
//! a later binder whose kind mentions one of those unknowns when the unknown's
//! own kind names a variable that is not in scope yet. The published kind spine
//! stays the denoted one. `KindScheme::binders` records the implicit-before-
//! explicit order and each binder's sort.

use super::super::{Checker, QUANTIFICATION_CHECK_FAILURE_IN_KIND};
use crate::denote::{KindScope, denote_kind};
use crate::kind::{Kind, KindBinder, flatten_spine, primitive_kind, type_kind};
use crate::solve::{substitute, unify_kind};
use psrs_hir::{self as hir, TypeId, TypeKind};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// A binder produced while elaborating one signature. Implicit unknowns use
/// the signature span; only a later explicit binder is reported.
struct Scoped {
    variable: u32,
    sort: Kind,
    span: TextRange,
    name: String,
}

impl Checker<'_> {
    /// The quantification order for `signature`.
    ///
    /// `explicit` is the rigid ids `parse_declared_kind` already allocated, in
    /// source order. On a signature this elaboration cannot check, the result
    /// is those binders with their written sorts and no implicit prefix, which
    /// is well scoped by the earlier reference check.
    pub(super) fn signature_binders(
        &mut self,
        id: TypeId,
        signature: &hir::Type,
        explicit: &[u32],
    ) -> Vec<KindBinder> {
        if explicit.is_empty() {
            return Vec::new();
        }
        let saved_state = self.state.clone();
        let saved_errors = self.errors.len();
        let elaborated = self.elaborate_signature(signature, explicit);
        let next = self.state.peek_next();
        self.state = saved_state;
        self.state.set_next(next);
        self.errors.truncate(saved_errors);
        match elaborated {
            Some(binders) => {
                self.report_ill_scoped(id, &binders);
                binders
                    .into_iter()
                    .map(|binder| KindBinder {
                        variable: binder.variable,
                        kind: binder.sort,
                    })
                    .collect()
            }
            None => self.denoted_binders(signature, explicit),
        }
    }

    fn report_ill_scoped(&mut self, id: TypeId, binders: &[Scoped]) {
        for binder in quantification_failures(binders) {
            self.report_about(
                id,
                QUANTIFICATION_CHECK_FAILURE_IN_KIND,
                binder.span,
                format!(
                    "the kind of `{}` depends on a variable that is not in scope yet",
                    binder.name
                ),
            );
        }
    }

    /// Infer binder sorts, instantiate constructors, and put unsolved unknowns
    /// before the explicit binders. `None` leaves the denoted scheme untouched.
    fn elaborate_signature(
        &mut self,
        signature: &hir::Type,
        explicit: &[u32],
    ) -> Option<Vec<Scoped>> {
        let (parameters, body) = leading_parameters(signature);
        if parameters.len() != explicit.len() {
            return None;
        }
        let mut unknown_sorts = HashMap::new();
        let mut scope: HashMap<String, (u32, Kind)> = HashMap::new();
        let mut elaborated = Vec::with_capacity(parameters.len());
        for (parameter, &variable) in parameters.iter().zip(explicit.iter()) {
            let sort = if let Some(annotation) = &parameter.kind {
                let denoted = self.denote_quiet(annotation, &identities(&scope))?;
                let annotation_kind = self.infer_kind(annotation, &scope, &mut unknown_sorts)?;
                self.unify_silent(annotation_kind, type_kind(), annotation.span)?;
                denoted
            } else {
                let sort_id = self.fresh_id();
                unknown_sorts.insert(sort_id, type_kind());
                Kind::Variable(sort_id)
            };
            scope.insert(parameter.name.clone(), (variable, sort.clone()));
            elaborated.push(Scoped {
                variable,
                sort,
                span: parameter.name_span,
                name: parameter.name.clone(),
            });
        }
        let body_kind = self.infer_kind(body, &scope, &mut unknown_sorts)?;
        self.unify_silent(body_kind, type_kind(), body.span)?;
        for binder in &mut elaborated {
            binder.sort = self.state.resolve(binder.sort.clone());
        }
        let mut binders =
            self.implicit_unknowns(&elaborated, explicit, &unknown_sorts, signature.span);
        binders.append(&mut elaborated);
        Some(binders)
    }

    fn implicit_unknowns(
        &self,
        explicit_binders: &[Scoped],
        explicit_ids: &[u32],
        unknown_sorts: &HashMap<u32, Kind>,
        span: TextRange,
    ) -> Vec<Scoped> {
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        for binder in explicit_binders {
            self.collect_unknown(
                binder.sort.clone(),
                explicit_ids,
                unknown_sorts,
                span,
                &mut seen,
                &mut found,
            );
        }
        found.sort_by_key(|binder| binder.variable);
        found
    }

    fn collect_unknown(
        &self,
        kind: Kind,
        explicit_ids: &[u32],
        unknown_sorts: &HashMap<u32, Kind>,
        span: TextRange,
        seen: &mut HashSet<u32>,
        found: &mut Vec<Scoped>,
    ) {
        match self.state.resolve(kind) {
            Kind::Variable(variable) => {
                if explicit_ids.contains(&variable)
                    || self.state.is_rigid(variable)
                    || !seen.insert(variable)
                {
                    return;
                }
                let sort = unknown_sorts
                    .get(&variable)
                    .cloned()
                    .unwrap_or_else(type_kind);
                let sort = self.state.resolve(sort);
                found.push(Scoped {
                    variable,
                    sort: sort.clone(),
                    span,
                    name: String::new(),
                });
                self.collect_unknown(sort, explicit_ids, unknown_sorts, span, seen, found);
            }
            Kind::App(function, argument) | Kind::Function(function, argument) => {
                self.collect_unknown(*function, explicit_ids, unknown_sorts, span, seen, found);
                self.collect_unknown(*argument, explicit_ids, unknown_sorts, span, seen, found);
            }
            Kind::RowExtend { field, tail, .. } => {
                self.collect_unknown(*field, explicit_ids, unknown_sorts, span, seen, found);
                self.collect_unknown(*tail, explicit_ids, unknown_sorts, span, seen, found);
            }
            Kind::ForAll { body, .. } => {
                self.collect_unknown(*body, explicit_ids, unknown_sorts, span, seen, found);
            }
            Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty | Kind::TypeVariable(_) => {}
        }
    }

    fn infer_kind(
        &mut self,
        ty: &hir::Type,
        scope: &HashMap<String, (u32, Kind)>,
        unknown_sorts: &mut HashMap<u32, Kind>,
    ) -> Option<Kind> {
        match &ty.kind {
            TypeKind::Variable(name) => {
                let (_, sort) = scope.get(name)?;
                Some(self.state.resolve(sort.clone()))
            }
            TypeKind::Constructor(builtin) => Some(primitive_kind(*builtin)),
            TypeKind::Named(id) | TypeKind::Opaque(id) => {
                self.instantiate_scheme(*id, unknown_sorts)
            }
            TypeKind::Application(..) => self.infer_application(ty, scope, unknown_sorts),
            TypeKind::Function { parameter, result } => {
                let parameter_kind = self.infer_kind(parameter, scope, unknown_sorts)?;
                self.unify_silent(parameter_kind, type_kind(), parameter.span)?;
                let result_kind = self.infer_kind(result, scope, unknown_sorts)?;
                self.unify_silent(result_kind, type_kind(), result.span)?;
                Some(type_kind())
            }
            TypeKind::Wildcard | TypeKind::Hole(_) => {
                let variable = self.fresh_id();
                unknown_sorts.insert(variable, type_kind());
                Some(Kind::Variable(variable))
            }
            TypeKind::Integer(_) => Some(Kind::Builtin(hir::BuiltinType::Int)),
            TypeKind::String(_) => Some(Kind::Builtin(hir::BuiltinType::Symbol)),
            TypeKind::Forall { .. }
            | TypeKind::Constrained { .. }
            | TypeKind::Row { .. }
            | TypeKind::Record { .. }
            | TypeKind::OperatorChain { .. }
            | TypeKind::Kinded { .. } => None,
        }
    }

    fn infer_application(
        &mut self,
        ty: &hir::Type,
        scope: &HashMap<String, (u32, Kind)>,
        unknown_sorts: &mut HashMap<u32, Kind>,
    ) -> Option<Kind> {
        let (head, arguments) = flatten_spine(ty);
        let head_kind = self.infer_kind(head, scope, unknown_sorts)?;
        let mut kind = self.peel_forall(head_kind, unknown_sorts);
        for argument in arguments {
            let resolved = self.state.resolve(kind);
            kind = self.peel_forall(resolved, unknown_sorts);
            let argument_kind = self.infer_kind(argument, scope, unknown_sorts)?;
            kind = match kind {
                Kind::Function(parameter, result) => {
                    self.unify_silent(*parameter, argument_kind, ty.span)?;
                    *result
                }
                other => {
                    let parameter = self.fresh_id();
                    let result = self.fresh_id();
                    unknown_sorts.insert(parameter, type_kind());
                    unknown_sorts.insert(result, type_kind());
                    self.unify_silent(
                        other,
                        Kind::Function(
                            Box::new(Kind::Variable(parameter)),
                            Box::new(Kind::Variable(result)),
                        ),
                        ty.span,
                    )?;
                    self.unify_silent(Kind::Variable(parameter), argument_kind, argument.span)?;
                    Kind::Variable(result)
                }
            };
        }
        Some(kind)
    }

    /// Instantiates a constructor's quantified binders, remembering each fresh
    /// variable's sort. A missing scheme aborts this elaboration instead of
    /// inventing a kind.
    fn instantiate_scheme(
        &mut self,
        id: TypeId,
        unknown_sorts: &mut HashMap<u32, Kind>,
    ) -> Option<Kind> {
        let scheme = self.schemes.get(&id).cloned()?;
        let mut mapping = HashMap::new();
        for binder in &scheme.binders {
            let fresh = self.fresh_id();
            let sort = substitute(&binder.kind, &mapping);
            mapping.insert(binder.variable, Kind::Variable(fresh));
            unknown_sorts.insert(fresh, sort);
        }
        for variable in &scheme.variables {
            if !mapping.contains_key(variable) {
                let fresh = self.fresh_id();
                mapping.insert(*variable, Kind::Variable(fresh));
                unknown_sorts.insert(fresh, type_kind());
            }
        }
        Some(substitute(&scheme.kind, &mapping))
    }

    fn peel_forall(&mut self, mut kind: Kind, unknown_sorts: &mut HashMap<u32, Kind>) -> Kind {
        loop {
            kind = self.state.resolve(kind);
            let Kind::ForAll { variable, body } = kind else {
                return kind;
            };
            let fresh = self.fresh_id();
            unknown_sorts.insert(fresh, type_kind());
            kind = substitute(&body, &HashMap::from([(variable, Kind::Variable(fresh))]));
        }
    }

    fn denote_quiet(&mut self, ty: &hir::Type, scope: &HashMap<String, Kind>) -> Option<Kind> {
        let mut kind_scope = KindScope::with_bindings(&mut self.state, scope.clone());
        denote_kind(ty, &mut kind_scope)
    }

    fn unify_silent(&mut self, left: Kind, right: Kind, span: TextRange) -> Option<()> {
        unify_kind(&mut self.state, left, right, span)
            .ok()
            .map(|_| ())
    }

    fn fresh_id(&mut self) -> u32 {
        match self.state.fresh() {
            Kind::Variable(id) => id,
            _ => unreachable!("a fresh kind is a variable"),
        }
    }

    /// Explicit binders only, each with the sort its annotation denotes.
    fn denoted_binders(&mut self, signature: &hir::Type, explicit: &[u32]) -> Vec<KindBinder> {
        let (parameters, _) = leading_parameters(signature);
        let mut scope = HashMap::new();
        let mut binders = Vec::new();
        for (parameter, variable) in parameters.iter().zip(explicit.iter().copied()) {
            let kind = match &parameter.kind {
                Some(annotation) => self
                    .denote_quiet(annotation, &scope)
                    .unwrap_or_else(type_kind),
                None => type_kind(),
            };
            scope.insert(parameter.name.clone(), Kind::Variable(variable));
            binders.push(KindBinder { variable, kind });
        }
        binders
    }
}

fn leading_parameters(ty: &hir::Type) -> (Vec<&hir::TypeParameter>, &hir::Type) {
    let mut parameters = Vec::new();
    let mut body = ty;
    while let TypeKind::Forall {
        variables,
        body: inner,
    } = &body.kind
    {
        parameters.extend(variables.iter());
        body = inner;
    }
    (parameters, body)
}

fn identities(scope: &HashMap<String, (u32, Kind)>) -> HashMap<String, Kind> {
    scope
        .iter()
        .map(|(name, (variable, _))| (name.clone(), Kind::Variable(*variable)))
        .collect()
}

/// `checkQuantification`: the first binder whose sort mentions a variable that
/// is not in scope yet contributes each later binder that depends on it.
fn quantification_failures(binders: &[Scoped]) -> Vec<&Scoped> {
    let mut scope = Vec::new();
    go(&mut scope, binders)
}

fn go<'a>(scope: &mut Vec<u32>, binders: &'a [Scoped]) -> Vec<&'a Scoped> {
    let Some((binder, rest)) = binders.split_first() else {
        return Vec::new();
    };
    let free = free_variables(&binder.sort);
    if free.iter().any(|variable| !scope.contains(variable)) {
        go_deps(binder.variable, rest)
    } else {
        scope.push(binder.variable);
        go(scope, rest)
    }
}

fn go_deps(dependency: u32, binders: &[Scoped]) -> Vec<&Scoped> {
    let mut failures = Vec::new();
    for binder in binders {
        let depends = free_variables(&binder.sort).contains(&dependency);
        if !depends {
            continue;
        }
        failures.push(binder);
        if binder.variable == dependency {
            break;
        }
    }
    failures
}

fn free_variables(kind: &Kind) -> Vec<u32> {
    fn walk(kind: &Kind, bound: &mut Vec<u32>, out: &mut Vec<u32>) {
        match kind {
            Kind::Variable(variable) if !bound.contains(variable) => out.push(*variable),
            Kind::Variable(_) => {}
            Kind::App(function, argument) | Kind::Function(function, argument) => {
                walk(function, bound, out);
                walk(argument, bound, out);
            }
            Kind::RowExtend { field, tail, .. } => {
                walk(field, bound, out);
                walk(tail, bound, out);
            }
            Kind::ForAll { variable, body } => {
                bound.push(*variable);
                walk(body, bound, out);
                bound.pop();
            }
            Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty | Kind::TypeVariable(_) => {}
        }
    }
    let mut out = Vec::new();
    walk(kind, &mut Vec::new(), &mut out);
    out.sort_unstable();
    out.dedup();
    out
}
