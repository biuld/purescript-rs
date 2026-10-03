use super::super::super::*;

impl Checker {
    /// Abstracts one dictionary parameter per retained constraint, in the order
    /// the obligations arose, and records each parameter as that constraint's
    /// solution.
    ///
    /// The parameter's type is the wanted constraint's own dictionary type, so
    /// the evidence the body uses and the parameter the scheme hands on are one
    /// dictionary rather than two elaborations of the same class that could
    /// differ in a fresh variable's identity.
    pub(in crate::typecheck) fn abstract_dictionaries(
        &mut self,
        residual: &[usize],
    ) -> Vec<(LocalId, InferType)> {
        residual
            .iter()
            .map(|&index| {
                let id = LocalId(self.state.next_dictionary_local);
                self.state.next_dictionary_local += 1;
                let Some(wanted) = self.state.wanted.get_mut(index) else {
                    return (id, InferType::RowEmpty);
                };
                let dictionary_type = wanted.dictionary_type.clone();
                wanted.solution = Some(WantedSolution::Abstracted(id));
                (id, dictionary_type)
            })
            .collect()
    }

    /// The wanted constraints a retained index set names, in the order the indices
    /// were given.
    pub(in crate::typecheck) fn residual_wanted(
        &self,
        residual: &[usize],
    ) -> Vec<WantedConstraint> {
        residual
            .iter()
            .filter_map(|&index| self.state.wanted.get(index).cloned())
            .collect()
    }

    /// The class constraints a retained wanted set denotes, in the order they
    /// arose and with the origin each obligation keeps.
    pub(in crate::typecheck) fn retained_constraints(
        &self,
        residual: &[usize],
    ) -> Vec<ClassConstraint> {
        residual
            .iter()
            .filter_map(|&index| self.state.wanted.get(index))
            .map(|wanted| ClassConstraint {
                class_id: wanted.class_id,
                arguments: wanted.arguments.clone(),
                span: wanted.span,
            })
            .collect()
    }

    /// Wraps a constrained body in one lambda per synthesized dictionary
    /// parameter so the declaration receives its dictionaries explicitly.
    pub(in crate::typecheck) fn wrap_dictionary_lambdas(
        &self,
        value: InferredExpr,
        parameters: &[(LocalId, InferType)],
    ) -> InferredExpr {
        let mut value = value;
        let mut ty = value.ty.clone();
        for (id, dictionary_type) in parameters.iter().rev() {
            ty = arrow(dictionary_type.clone(), ty);
            let span = value.span;
            value = InferredExpr {
                kind: InferredExprKind::Lambda {
                    binder: InferredBinder {
                        binder: LocalBinder {
                            id: *id,
                            name: "dict".into(),
                            span,
                        },
                        scheme: Scheme::monomorphic(dictionary_type.clone()),
                    },
                    body: Box::new(value),
                },
                ty: ty.clone(),
                span,
            };
        }
        value
    }

    /// Builds the THIR evidence for a solved wanted constraint.
    pub(in crate::typecheck) fn wanted_evidence(
        &mut self,
        index: usize,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::Evidence> {
        let wanted = self.state.wanted.get(index)?.clone();
        self.constraint_evidence(&wanted, interner, generics)
    }

    fn constraint_evidence(
        &mut self,
        constraint: &WantedConstraint,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::Evidence> {
        let solution = constraint.solution.clone()?;
        let ty = self.finalize_type(
            &constraint.dictionary_type,
            constraint.span,
            interner,
            generics,
        )?;
        Some(thir::Evidence {
            kind: self.solution_kind(solution, constraint.span, interner, generics)?,
            class_id: constraint.class_id,
            ty,
            span: constraint.span,
        })
    }

    fn solution_kind(
        &mut self,
        solution: WantedSolution,
        span: TextRange,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::EvidenceKind> {
        Some(match solution {
            WantedSolution::Given(id) | WantedSolution::Abstracted(id) => {
                thir::EvidenceKind::Given(id)
            }
            WantedSolution::Global(symbol) => thir::EvidenceKind::Global(symbol),
            WantedSolution::Instance {
                constructor,
                constructor_type,
                context,
            } => {
                let constructor_type =
                    self.finalize_type(&constructor_type, span, interner, generics)?;
                let mut evidence = Vec::with_capacity(context.len());
                for child_id in &context {
                    let Some(child) = self
                        .state
                        .wanted
                        .iter()
                        .find(|wanted| wanted.id == *child_id)
                        .cloned()
                    else {
                        self.state.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::InvalidHir,
                            span,
                            format!(
                                "instance evidence refers to missing wanted constraint {child_id}"
                            ),
                        ));
                        return None;
                    };
                    evidence.push(self.constraint_evidence(&child, interner, generics)?);
                }
                thir::EvidenceKind::Instance {
                    constructor,
                    constructor_type,
                    context: evidence,
                }
            }
            WantedSolution::Superclass { parent, field } => {
                let parent = self.constraint_evidence(&parent, interner, generics)?;
                thir::EvidenceKind::Superclass {
                    parent: Box::new(parent),
                    field,
                }
            }
            WantedSolution::Coercible { source, target } => thir::EvidenceKind::Coercible {
                source_type: self.finalize_type(&source, span, interner, generics)?,
                target_type: self.finalize_type(&target, span, interner, generics)?,
            },
            // A `Prim` relation's dictionary erases, so what the evidence carries
            // is the decision: the arguments the rule fixed, finalized through the
            // shared finalizer so they are the arguments the constraint now has.
            WantedSolution::Primitive { arguments } => thir::EvidenceKind::Primitive {
                arguments: arguments
                    .iter()
                    .map(|argument| self.finalize_type(argument, span, interner, generics))
                    .collect::<Option<Vec<_>>>()?,
            },
        })
    }
}
