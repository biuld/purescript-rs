//! Inference of a module's declarations, one binding group at a time.
//!
//! `typesOf` in official PureScript is the reference for this sequence: expose
//! the group's declared signatures at its recursive uses, infer or check every
//! body against the signature or nothing at all, solve the wanteds that a given,
//! a superclass path, an instance, or a primitive relation discharges, retain
//! the rest as the declaration's scheme constraints, check that each retained
//! variable is determined, generalize the type and the constraints together, and
//! abstract one dictionary parameter per retained constraint.
//!
//! The line between solving and retaining is the binding group's recursion, not
//! the solver: a non-recursive group retains what it could not discharge, and a
//! recursive one reports it, because a generalized constraint the recursive
//! uses never proved is unsound polymorphic recursion.

use super::classes::UnsolvedPolicy;
use super::*;

/// What one declaration of a binding group still has to decide once its body has
/// been checked. It is the group's own state: the solver keeps its
/// substitutions, wanteds, and evidence, and the entry point keeps only the
/// finished declarations.
struct GroupMember {
    /// The declaration's index in the module's declaration list.
    index: usize,
    symbol: SymbolId,
    /// Whether the source declared a signature. A declaration with one may only
    /// discharge its constraints from its own dictionary parameters; a
    /// declaration without one generalizes over what it could not discharge.
    declared: bool,
    /// The variables the signature's own `forall` binders introduced, which its
    /// scheme quantifies however the solver levelled them.
    quantified: Vec<u32>,
    /// The dictionary parameters the declaration abstracts, one per retained
    /// constraint, in the order the obligations arose.
    parameters: Vec<(LocalId, InferType)>,
    /// The checked body, before the dictionary lambdas wrap around it.
    value: Option<InferredExpr>,
    /// The indices of the wanteds this declaration could not discharge. They
    /// become the declaration's scheme constraints.
    residual: Vec<usize>,
    /// Whether `self::residual_wanted` was called for this declaration, which
    /// re-solves the group's other retained obligations before taking them. It is
    /// what lets one member of a group determine an obligation another member
    /// raised, and it must happen at most once per declaration.
    residual_resolved: bool,
}

impl Checker {
    /// Infers and generalizes every declaration of a module, one binding group at
    /// a time in dependency order, so each group is generalized before the groups
    /// that use it.
    pub(super) fn infer_declarations(
        &mut self,
        module: &hir::Module,
        inferred: &mut [Option<InferredDeclaration>],
    ) {
        let mut annotation_scopes = vec![HashMap::new(); module.declarations.len()];
        for group in order::declaration_order(module) {
            let mut members = group
                .members
                .iter()
                .map(|&index| {
                    self.expose_declaration(
                        index,
                        &module.declarations[index],
                        &mut annotation_scopes[index],
                    )
                })
                .collect::<Vec<_>>();
            for (member, &index) in members.iter_mut().zip(&group.members) {
                self.check_declaration_body(
                    &module.declarations[index],
                    member,
                    group.recursive,
                    &annotation_scopes[index],
                );
            }
            for (member, &index) in members.iter_mut().zip(&group.members) {
                self.generalize_declaration(&module.declarations[index], member, inferred);
            }
        }
    }

    /// Places a declaration in the group's environment before any body is
    /// checked, so a recursive use sees a scheme it can instantiate.
    ///
    /// A declaration with a signature is exposed as that signature, with the
    /// signature's own `forall` binders quantified: a use of it instantiates them
    /// rather than sharing one rigid variable across the group. A declaration
    /// without one is exposed as the fresh monotype its body will be checked
    /// against, which is what lets the group refer to itself at all.
    fn expose_declaration(
        &mut self,
        index: usize,
        declaration: &hir::Declaration,
        annotation_variables: &mut HashMap<String, InferType>,
    ) -> GroupMember {
        let (scheme, parameters, quantified, declared) = match &declaration.signature {
            Some(signature) => {
                let signature = self.elaborate_declaration_signature(signature);
                *annotation_variables = signature.annotation_variables.clone();
                let scheme = self.declared_scheme(
                    &signature.quantified,
                    signature.constraints,
                    signature.ty,
                );
                (scheme, signature.parameters, signature.quantified, true)
            }
            None => (
                Scheme::monomorphic(self.fresh()),
                Vec::new(),
                Vec::new(),
                false,
            ),
        };
        self.state
            .pending_signatures
            .insert(declaration.symbol, parameters.clone());
        self.scope.globals.insert(declaration.symbol, scheme);
        GroupMember {
            index,
            symbol: declaration.symbol,
            declared,
            quantified,
            parameters,
            value: None,
            residual: Vec::new(),
            residual_resolved: false,
        }
    }

    /// Checks one declaration's body against its declared type, or infers one
    /// when the source declared none, then splits its wanted constraints into
    /// the ones that are discharged and the ones its scheme retains.
    ///
    /// `recursive` is the group's own recursion: it decides whether an obligation
    /// the declaration could not discharge is retained for generalization or
    /// reported.
    fn check_declaration_body(
        &mut self,
        declaration: &hir::Declaration,
        member: &mut GroupMember,
        recursive: bool,
        annotation_variables: &HashMap<String, InferType>,
    ) {
        let scheme = self.scope.globals[&declaration.symbol].clone();
        let parameters = self
            .state
            .pending_signatures
            .get(&declaration.symbol)
            .cloned()
            .unwrap_or_default();
        let declared = member.declared;
        self.scope.annotation_variables = annotation_variables.clone();
        // A declared signature may only discharge its constraints from its own
        // dictionary parameters, so an obligation left over is a missing
        // instance. A declaration without one retains what it could not
        // discharge, which a non-recursive group generalizes and a recursive one
        // reports.
        let unsolved = if declared {
            UnsolvedPolicy::RequireSolved
        } else {
            UnsolvedPolicy::Retain
        };
        let body = self.with_scope(|checker| {
            checker.begin_givens(&scheme.constraints, &parameters);
            let wanted_start = checker.state.wanted.len();
            let expected = declaration
                .signature
                .as_ref()
                .map(|_| checker.scope.globals[&declaration.symbol].ty.clone());
            let value = checker.infer_expr_with_expected(&declaration.value, expected);
            let Some(value) = value else {
                checker.end_givens();
                return None;
            };
            let span = declaration
                .signature
                .as_ref()
                .map_or(declaration.name_span, |signature| signature.span);
            checker.unify(scheme.ty.clone(), value.ty.clone(), span);
            // A declared signature may name ambiguous variables for the caller to
            // instantiate, so only an inferred binding is measured against its
            // own result type.
            let result = (!declared).then(|| value.ty.clone());
            member.residual =
                checker.solve_wanted_constraints(result.as_ref(), wanted_start, unsolved);
            checker.end_givens();
            Some(value)
        });
        let Some(value) = body else {
            return;
        };
        if recursive && !declared && !member.residual.is_empty() {
            // Generalizing a recursive group's retained constraints would admit
            // polymorphic recursion over a constraint the recursive uses never
            // proved, so they are reported instead.
            self.report_ungeneralizable_recursion(declaration, &member.residual);
            member.residual.clear();
        }
        member.value = Some(value);
    }

    /// Reports the constraints a recursive declaration could not discharge and
    /// therefore cannot generalize over, at the declaration's own range.
    fn report_ungeneralizable_recursion(
        &mut self,
        declaration: &hir::Declaration,
        residual: &[usize],
    ) {
        let constraints = self
            .retained_constraints(residual)
            .iter()
            .map(|constraint| self.display_constraint(constraint.class_id, &constraint.arguments))
            .collect::<Vec<_>>()
            .join(", ");
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::CannotGeneralizeRecursiveFunction,
            declaration.name_span,
            format!(
                "unable to generalize the type of the recursive function `{}`: a recursive binding group must prove its constraints ({constraints}), so add a type signature",
                declaration.name
            ),
        ));
    }

    /// Generalizes one declaration: checks that its retained constraints are
    /// determined, quantifies the type and the constraints together, and wraps
    /// the body in one lambda per retained constraint.
    fn generalize_declaration(
        &mut self,
        declaration: &hir::Declaration,
        member: &mut GroupMember,
        inferred: &mut [Option<InferredDeclaration>],
    ) {
        let Some(value) = member.value.clone() else {
            return;
        };
        let monomorphic = self.scope.globals[&member.symbol].ty.clone();
        // A declared signature states its own constraints, and its dictionary
        // parameters already discharge them. An inferred declaration's scheme
        // carries exactly the obligations it could not discharge.
        let retained = if member.declared {
            self.scope.globals[&member.symbol].constraints.clone()
        } else {
            // One member of the group may determine an obligation another member
            // raised, so the group's retained obligations get one more attempt
            // before anything is measured against them.
            if !member.residual_resolved && !member.residual.is_empty() {
                member.residual_resolved = true;
                self.speculate(|checker| {
                    checker.solve_wanted_constraints(
                        None,
                        checker.state.wanted.len(),
                        UnsolvedPolicy::Retain,
                    );
                    Some(())
                });
            }
            // One dictionary parameter per retained constraint, in source order.
            // The wanteds keep their own indices, so the evidence the body already
            // refers to now names these parameters.
            member.parameters = self.abstract_dictionaries(&member.residual);
            self.check_residual_ambiguity(
                &self.residual_wanted(&member.residual),
                &value.ty,
                &declaration.name,
                declaration.name_span,
            );
            self.retained_constraints(&member.residual)
        };
        let scheme = self.generalize(&member.quantified, &monomorphic, &retained, TOP_LEVEL);
        let value = self.wrap_dictionary_lambdas(value, &member.parameters);
        self.state
            .pending_signatures
            .insert(member.symbol, member.parameters.clone());
        self.scope.globals.insert(member.symbol, scheme.clone());
        inferred[member.index] = Some(InferredDeclaration {
            symbol: member.symbol,
            name: declaration.name.clone(),
            name_span: declaration.name_span,
            scheme,
            value,
            span: declaration.span,
        });
    }
}
