use super::super::signature::{flatten_spine, nominal_type_id};
use super::super::unify::substitute;
use super::super::*;

impl Checker {
    /// Elaborates one constraint type `C τ...` into a class constraint with
    /// instantiated type arguments.
    pub(in crate::typecheck) fn elaborate_constraint(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
        rigid: bool,
    ) -> Option<ClassConstraint> {
        let (head, arguments) = flatten_spine(ty);
        let Some(class_id) = nominal_type_id(head) else {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                ty.span,
                "a constraint must name a class",
            ));
            return None;
        };
        let Some(class) = self.env.classes.get(&class_id).cloned() else {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                ty.span,
                "a constraint names an unknown class",
            ));
            return None;
        };
        if arguments.len() != class.parameters.len() {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                ty.span,
                "a class constraint has the wrong number of type arguments",
            ));
            return None;
        }
        let arguments = arguments
            .iter()
            .map(|argument| self.elaborate_type_mode(argument, variables, rigid))
            .collect();
        Some(ClassConstraint {
            class_id,
            arguments,
            span: ty.span,
        })
    }

    /// The dictionary record type for a class constraint: one field per
    /// superclass (holding that superclass's dictionary) followed by one field
    /// per method, with the class parameters substituted.
    pub(in crate::typecheck) fn dictionary_type(
        &mut self,
        constraint: &ClassConstraint,
    ) -> InferType {
        let Some(class) = self.env.classes.get(&constraint.class_id).cloned() else {
            return record_type(Vec::new(), InferType::RowEmpty);
        };
        let mut variables = HashMap::new();
        for (parameter, argument) in class.parameters.iter().zip(&constraint.arguments) {
            variables.insert(parameter.clone(), argument.clone());
        }
        let mut fields = Vec::with_capacity(class.superclasses.len() + class.methods.len());
        // Each method's field type is generalized against the level the
        // dictionary is built at, not against the nested level its signature is
        // elaborated in.
        let outer_level = self.state.level;
        for (field, super_constraint) in
            self.superclass_constraints(constraint.class_id, &constraint.arguments)
        {
            let field_ty = self.dictionary_type(&super_constraint);
            fields.push((field, field_ty));
        }
        for method in &class.methods {
            let mut method_variables = variables.clone();
            let field_ty = self.in_nested_level(|checker| {
                checker.elaborate_type_mode(&method.signature, &mut method_variables, false)
            });
            let field_ty = self.generalize(&[], &field_ty, &[], outer_level).ty;
            fields.push((method.name.clone(), field_ty));
        }
        record_type(fields, InferType::RowEmpty)
    }

    /// Types a class method at a use site. The class parameters become fresh
    /// variables and the resulting constraint becomes a wanted dictionary.
    pub(in crate::typecheck) fn infer_method_use(
        &mut self,
        class_id: hir::TypeId,
        method: MethodInfo,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let class = self.env.classes.get(&class_id).cloned()?;
        let mut variables = HashMap::new();
        let mut arguments = Vec::with_capacity(class.parameters.len());
        for parameter in &class.parameters {
            let variable = self.fresh();
            variables.insert(parameter.clone(), variable.clone());
            arguments.push(variable);
        }
        let ty = self.elaborate_type_mode(&method.signature, &mut variables, false);
        let constraint = ClassConstraint {
            class_id,
            arguments,
            span,
        };
        let dictionary_type = self.dictionary_type(&constraint);
        if let Some(dictionary_method_type) = record_field_type(&dictionary_type, &method.name) {
            self.unify(ty.clone(), dictionary_method_type, span);
        }
        let wanted = self.push_wanted(constraint, dictionary_type);
        Some((
            InferredExprKind::Method {
                method: method.name,
                wanted,
            },
            ty,
        ))
    }

    /// Applies one dictionary argument per scheme constraint to a global or
    /// local use, inserting a wanted constraint for each.
    pub(in crate::typecheck) fn apply_constraints(
        &mut self,
        base: InferredExpr,
        constraints: Vec<ClassConstraint>,
        span: TextRange,
    ) -> InferredExpr {
        if constraints.is_empty() {
            return base;
        }
        let mut dictionary_types = Vec::with_capacity(constraints.len());
        let mut shapes = Vec::with_capacity(constraints.len() + 1);
        let mut current = base.ty.clone();
        shapes.push(current.clone());
        for constraint in constraints.iter().rev() {
            let dictionary_type = self.dictionary_type(constraint);
            dictionary_types.push(dictionary_type.clone());
            current = arrow(dictionary_type, current);
            shapes.push(current.clone());
        }
        dictionary_types.reverse();
        shapes.reverse();
        let mut expression = InferredExpr {
            kind: base.kind,
            ty: shapes[0].clone(),
            span,
        };
        for (index, constraint) in constraints.into_iter().enumerate() {
            let dictionary_type = dictionary_types[index].clone();
            let wanted = self.push_wanted(constraint, dictionary_type);
            expression = InferredExpr {
                kind: InferredExprKind::DictionaryApplication {
                    function: Box::new(expression),
                    wanted,
                },
                ty: shapes[index + 1].clone(),
                span,
            };
        }
        expression
    }

    /// Instantiates a scheme's quantified variables and returns its constraints
    /// together with its (constraint-free) type.
    pub(in crate::typecheck) fn instantiate_use(
        &mut self,
        scheme: &Scheme,
    ) -> (Vec<ClassConstraint>, InferType) {
        let mapping = self
            .instantiate_type_variables(scheme.variables.iter().copied(), &scheme.variable_kinds);
        let substituted = substitute(&scheme.ty, &mapping);
        let freshened = self.freshen_foralls(&substituted);
        let mut ty = self.resolve_type(freshened);
        // A value whose result or local binding type begins with a structural
        // forall is instantiated independently at each occurrence.
        let mut nested_constraints = Vec::new();
        loop {
            match ty {
                InferType::ForAll { variables, body } => {
                    let quantified = self.instantiate_type_variables(variables, &HashMap::new());
                    ty = self.resolve_type(substitute(&body, &quantified));
                }
                InferType::Constrained { constraints, body } => {
                    nested_constraints.extend(constraints);
                    ty = self.resolve_type(*body);
                }
                _ => break,
            }
        }
        let mut constraints: Vec<ClassConstraint> = scheme
            .constraints
            .iter()
            .map(|constraint| ClassConstraint {
                class_id: constraint.class_id,
                arguments: constraint
                    .arguments
                    .iter()
                    .map(|argument| substitute(argument, &mapping))
                    .collect(),
                span: constraint.span,
            })
            .collect();
        constraints.extend(nested_constraints);
        (constraints, ty)
    }

    pub(in crate::typecheck) fn push_wanted(
        &mut self,
        constraint: ClassConstraint,
        dictionary_type: InferType,
    ) -> usize {
        let index = self.state.wanted.len();
        self.state.wanted.push(WantedConstraint {
            class_id: constraint.class_id,
            arguments: constraint.arguments,
            dictionary_type,
            span: constraint.span,
            givens: self.scope.givens.clone(),
            solution: None,
        });
        index
    }

    /// Makes the declaration's dictionary parameters available as given
    /// evidence while its body is checked.
    pub(in crate::typecheck) fn begin_givens(
        &mut self,
        constraints: &[ClassConstraint],
        parameters: &[(LocalId, InferType)],
    ) {
        self.scope.givens.clear();
        self.scope.given_rigid.clear();
        for (constraint, (id, _)) in constraints.iter().zip(parameters) {
            for argument in &constraint.arguments {
                let mut variables = HashSet::new();
                super::fundeps::collect_infer_variables(argument, &mut variables);
                for variable in variables {
                    if self.state.rigid.insert(variable) {
                        self.scope.given_rigid.push(variable);
                    }
                }
            }
            self.scope
                .givens
                .push((constraint.clone(), WantedSolution::Given(*id)));
        }
    }

    pub(in crate::typecheck) fn end_givens(&mut self) {
        self.scope.givens.clear();
        for variable in self.scope.given_rigid.drain(..) {
            self.state.rigid.remove(&variable);
        }
    }

    /// Structural equality of two resolved inference types. Distinct unsolved
    /// variables are not equal, so ambiguity never selects an instance.
    pub(in crate::typecheck) fn infer_types_equal(
        &self,
        left: &InferType,
        right: &InferType,
    ) -> bool {
        match (
            self.resolve_type(left.clone()),
            self.resolve_type(right.clone()),
        ) {
            (InferType::Variable(a), InferType::Variable(b)) => a == b,
            (InferType::Constructor(a), InferType::Constructor(b)) => a == b,
            (InferType::Application(f1, a1), InferType::Application(f2, a2)) => {
                self.infer_types_equal(&f1, &f2) && self.infer_types_equal(&a1, &a2)
            }
            (InferType::RowEmpty, InferType::RowEmpty) => true,
            (
                InferType::RowExtend {
                    label: l1,
                    ty: t1,
                    tail: r1,
                },
                InferType::RowExtend {
                    label: l2,
                    ty: t2,
                    tail: r2,
                },
            ) => l1 == l2 && self.infer_types_equal(&t1, &t2) && self.infer_types_equal(&r1, &r2),
            _ => false,
        }
    }

    pub(in crate::typecheck) fn display_constraint(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> String {
        let name = self
            .env
            .type_names
            .get(&class_id)
            .cloned()
            .unwrap_or_else(|| "?".into());
        let arguments = arguments
            .iter()
            .map(|argument| self.display_type(argument))
            .collect::<Vec<_>>()
            .join(" ");
        if arguments.is_empty() {
            name
        } else {
            format!("{name} {arguments}")
        }
    }

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
                for child in &context {
                    evidence.push(self.constraint_evidence(child, interner, generics)?);
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

pub(super) fn record_field_type(record: &InferType, wanted: &str) -> Option<InferType> {
    let mut row = super::super::record_row(record)?;
    loop {
        match row {
            InferType::RowExtend { label, ty, .. } if label == wanted => return Some(*ty),
            InferType::RowExtend { tail, .. } => row = *tail,
            InferType::RowEmpty => return None,
            _ => return None,
        }
    }
}
