//! One instantiation rule for quantified types.
//!
//! A binder is visible when its source wrote `@`. Ordinary instantiation
//! in a consuming context opens only an invisible prefix. A visible application selects the next
//! visible binder and leaves the rest quantified. Class parameters are that
//! visible prefix on a method type; they are not a side table.

use super::unify::substitute;
use super::*;

impl Checker {
    /// Splits quantified variables at the first visible binder.
    ///
    /// The prefix is instantiated to unknowns. The remainder, including any
    /// invisible binder that follows a visible one, stays quantified.
    /// Constraints that mention the remainder stay inside that quantifier.
    pub(in crate::typecheck) fn reassociate_quantifiers(
        &mut self,
        quantified: Vec<u32>,
        visible: Vec<bool>,
        constraints: Vec<ClassConstraint>,
        ty: InferType,
    ) -> (Vec<ClassConstraint>, InferType) {
        let split = first_visible(&visible, quantified.len());
        let rest = quantified[split..].to_vec();
        let rest_visible = visible_flags(&visible[split.min(visible.len())..], rest.len());
        let rest_ids = rest.iter().copied().collect::<HashSet<_>>();
        let (kept, emitted) = partition_constraints(constraints, &rest_ids);
        for id in &rest {
            self.state.rigid.insert(*id);
        }
        (emitted, quantify(rest, rest_visible, kept, ty))
    }

    /// Freshens a variable's existing scheme without consuming its binders.
    /// Synthesis retains the quantified type; the surrounding application,
    /// monomorphic check or explicit type application decides what to consume.
    pub(in crate::typecheck) fn synthesize_scheme_type(&mut self, scheme: &Scheme) -> InferType {
        let mapping = self.freshen_binders(&scheme.variables, &scheme.variable_kinds);
        let variables = scheme
            .variables
            .iter()
            .map(|id| match mapping[id] {
                InferType::Variable(fresh) => fresh,
                _ => unreachable!("freshened scheme binders are variables"),
            })
            .collect();
        let constraints = scheme
            .constraints
            .iter()
            .map(|constraint| ClassConstraint {
                arguments: constraint
                    .arguments
                    .iter()
                    .map(|ty| substitute(ty, &mapping))
                    .collect(),
                ..constraint.clone()
            })
            .collect();
        let ty = self.freshen_foralls(&substitute(&scheme.ty, &mapping));
        quantify(
            variables,
            visible_flags(&scheme.visible, scheme.variables.len()),
            constraints,
            ty,
        )
    }

    /// Instantiates a scheme at a use.
    ///
    /// Invisible scheme variables become unknowns. The first visible variable
    /// and everything after it are freshened and returned as a `forall`, so a
    /// later visible application can still select them. Structural quantifiers
    /// at the head follow the same rule.
    pub(in crate::typecheck) fn instantiate_use(
        &mut self,
        scheme: &Scheme,
    ) -> (Vec<ClassConstraint>, InferType) {
        let flags = visible_flags(&scheme.visible, scheme.variables.len());
        let split = first_visible(&flags, scheme.variables.len());
        let prefix_map = self.instantiate_type_variables(
            scheme.variables[..split].iter().copied(),
            &scheme.variable_kinds,
        );
        let rest_map = self.freshen_binders(&scheme.variables[split..], &scheme.variable_kinds);
        let mut mapping = prefix_map;
        let rest_ids = rest_map
            .values()
            .filter_map(|ty| match ty {
                InferType::Variable(id) => Some(*id),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let rest_variables = scheme.variables[split..]
            .iter()
            .filter_map(|variable| match rest_map.get(variable) {
                Some(InferType::Variable(id)) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>();
        let rest_visible = visible_flags(&flags[split..], rest_variables.len());
        mapping.extend(rest_map);
        let substituted = self.freshen_foralls(&substitute(&scheme.ty, &mapping));
        let constraints = scheme
            .constraints
            .iter()
            .map(|constraint| ClassConstraint {
                arguments: constraint
                    .arguments
                    .iter()
                    .map(|argument| substitute(argument, &mapping))
                    .collect(),
                ..constraint.clone()
            })
            .collect::<Vec<_>>();
        let (kept, mut emitted) = partition_constraints(constraints, &rest_ids);
        let mut ty = self.resolve_type(quantify(rest_variables, rest_visible, kept, substituted));
        if rest_ids.is_empty() {
            let (nested, peeled) = self.peel_until_visible(ty);
            emitted.extend(nested);
            ty = peeled;
        }
        (emitted, self.resolve_type(ty))
    }

    /// Peels every leading quantifier and constraint, emitting a wanted for
    /// each constraint. Value application and a check against a monotype use
    /// this; visibility does not stop it.
    pub(in crate::typecheck) fn instantiate_all_expr(
        &mut self,
        expression: InferredExpr,
        span: TextRange,
    ) -> InferredExpr {
        let (binding, mut expression) = self.prepare_expression_use(expression);
        loop {
            let ty = self.resolve_type(expression.ty.clone());
            match ty {
                InferType::ForAll {
                    variables, body, ..
                } => {
                    let mapping = self.instantiate_type_variables(variables, &HashMap::new());
                    expression.ty = self.resolve_type(substitute(&body, &mapping));
                }
                InferType::Constrained { constraints, body } => {
                    expression.ty = *body;
                    expression = self.apply_constraints(expression, constraints, span);
                }
                _ => break,
            }
        }
        self.finish_expression_use(binding, expression)
    }

    /// Instantiates a leading invisible quantifier prefix and stops at the
    /// first visible binder. Constraints are left for the caller.
    pub(in crate::typecheck) fn instantiate_until_visible_expr(
        &mut self,
        expression: InferredExpr,
    ) -> InferredExpr {
        let (binding, mut expression) = self.prepare_expression_use(expression);
        let span = expression.span;
        let (constraints, ty) = self.peel_until_visible(self.resolve_type(expression.ty.clone()));
        expression.ty = ty;
        // An invisible prefix may expose constraints independent of the
        // still-visible binders. Apply them to the reference, retaining its value.
        let expression = self.apply_constraints(expression, constraints, span);
        self.finish_expression_use(binding, expression)
    }

    /// `instantiateConstraint`: peel leading constraints and stop at the next
    /// quantifier or monotype.
    pub(in crate::typecheck) fn instantiate_head_constraints(
        &mut self,
        expression: InferredExpr,
        span: TextRange,
    ) -> InferredExpr {
        let (binding, mut expression) = self.prepare_expression_use(expression);
        loop {
            let ty = self.resolve_type(expression.ty.clone());
            let InferType::Constrained { constraints, body } = ty else {
                break;
            };
            expression.ty = *body;
            expression = self.apply_constraints(expression, constraints, span);
        }
        self.finish_expression_use(binding, expression)
    }

    /// The leading quantifiers and constraints of an elaborated method type.
    pub(in crate::typecheck) fn split_quantifier_prefix(
        ty: InferType,
    ) -> (Vec<u32>, Vec<bool>, Vec<ClassConstraint>, InferType) {
        let mut variables = Vec::new();
        let mut visible = Vec::new();
        let mut constraints = Vec::new();
        let mut ty = ty;
        loop {
            match ty {
                InferType::ForAll {
                    variables: binders,
                    visible: flags,
                    body,
                } => {
                    for (index, variable) in binders.into_iter().enumerate() {
                        visible.push(flags.get(index).copied().unwrap_or(false));
                        variables.push(variable);
                    }
                    ty = *body;
                }
                InferType::Constrained {
                    constraints: more,
                    body,
                } => {
                    constraints.extend(more);
                    ty = *body;
                }
                other => return (variables, visible, constraints, other),
            }
        }
    }

    fn peel_until_visible(&mut self, mut ty: InferType) -> (Vec<ClassConstraint>, InferType) {
        let mut emitted = Vec::new();
        loop {
            ty = self.resolve_type(ty);
            match ty {
                InferType::ForAll {
                    variables,
                    visible,
                    body,
                } => {
                    let count = first_visible(&visible, variables.len());
                    if count == 0 {
                        ty = InferType::ForAll {
                            variables,
                            visible,
                            body,
                        };
                        break;
                    }
                    let mapping = self.instantiate_type_variables(
                        variables.iter().copied().take(count),
                        &HashMap::new(),
                    );
                    let mut rest = substitute(&body, &mapping);
                    if count < variables.len() {
                        rest = InferType::ForAll {
                            variables: variables[count..].to_vec(),
                            visible: visible_flags(&visible[count..], variables.len() - count),
                            body: Box::new(rest),
                        };
                        ty = self.resolve_type(rest);
                        break;
                    }
                    ty = self.resolve_type(rest);
                }
                InferType::Constrained { constraints, body } => {
                    emitted.extend(constraints);
                    ty = *body;
                }
                other => {
                    ty = other;
                    break;
                }
            }
        }
        (emitted, self.resolve_type(ty))
    }

    /// Fresh binder identities for quantifiers a use has not instantiated.
    /// Source names move with them, and the new binders are rigid until a
    /// later rule instantiates them. Kind variables inside the recorded kind
    /// are instantiated once for this use and shared across its binders, so
    /// `Proxy @"hi"` and `Proxy @Array` each choose their own `k`.
    fn freshen_binders(
        &mut self,
        variables: &[u32],
        kinds: &HashMap<u32, Kind>,
    ) -> HashMap<u32, InferType> {
        let mut mapping = HashMap::new();
        let mut kind_mapping = HashMap::new();
        for variable in variables {
            let InferType::Variable(fresh) = self.fresh() else {
                unreachable!("fresh inference types are variables")
            };
            self.state.rigid.insert(fresh);
            let kind = kinds
                .get(variable)
                .cloned()
                .or_else(|| self.recorded_kind(*variable));
            if let Some(kind) = kind {
                let kind = self.instantiate_kind(kind, &mut kind_mapping);
                self.record_variable_kind(fresh, kind);
            }
            if let Some(name) = self.scope.type_variable_names.get(variable).cloned() {
                self.scope.type_variable_names.insert(fresh, name);
            }
            mapping.insert(*variable, InferType::Variable(fresh));
        }
        mapping
    }
}

fn first_visible(visible: &[bool], len: usize) -> usize {
    (0..len)
        .find(|index| visible.get(*index).copied().unwrap_or(false))
        .unwrap_or(len)
}

fn visible_flags(visible: &[bool], len: usize) -> Vec<bool> {
    (0..len)
        .map(|index| visible.get(index).copied().unwrap_or(false))
        .collect()
}

fn quantify(
    variables: Vec<u32>,
    visible: Vec<bool>,
    constraints: Vec<ClassConstraint>,
    ty: InferType,
) -> InferType {
    let mut body = ty;
    if !constraints.is_empty() {
        body = InferType::Constrained {
            constraints,
            body: Box::new(body),
        };
    }
    if variables.is_empty() {
        body
    } else {
        InferType::ForAll {
            variables,
            visible,
            body: Box::new(body),
        }
    }
}

fn partition_constraints(
    constraints: Vec<ClassConstraint>,
    rest: &HashSet<u32>,
) -> (Vec<ClassConstraint>, Vec<ClassConstraint>) {
    let mut kept = Vec::new();
    let mut emitted = Vec::new();
    for constraint in constraints {
        let mentions = constraint
            .arguments
            .iter()
            .any(|argument| type_mentions(argument, rest));
        if mentions {
            kept.push(constraint);
        } else {
            emitted.push(constraint);
        }
    }
    (kept, emitted)
}

fn type_mentions(ty: &InferType, ids: &HashSet<u32>) -> bool {
    match ty {
        InferType::Variable(id) => ids.contains(id),
        InferType::Application(function, argument) => {
            type_mentions(function, ids) || type_mentions(argument, ids)
        }
        InferType::ForAll {
            variables, body, ..
        } => {
            let mut ids = ids.clone();
            for variable in variables {
                ids.remove(variable);
            }
            type_mentions(body, &ids)
        }
        InferType::Constrained { constraints, body } => {
            constraints.iter().any(|constraint| {
                constraint
                    .arguments
                    .iter()
                    .any(|argument| type_mentions(argument, ids))
            }) || type_mentions(body, ids)
        }
        InferType::RowExtend { ty, tail, .. } => type_mentions(ty, ids) || type_mentions(tail, ids),
        InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => false,
    }
}
