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
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                ty.span,
                "a constraint must name a class",
            ));
            return None;
        };
        let Some(class) = self.classes.get(&class_id).cloned() else {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                ty.span,
                "a constraint names an unknown class",
            ));
            return None;
        };
        if class.superclasses != 0 {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                ty.span,
                "constraints on classes with superclasses are not supported yet",
            ));
            return None;
        }
        if class.parameters.len() != 1 {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                ty.span,
                "constraints on multi-parameter classes are not supported yet",
            ));
            return None;
        }
        if arguments.len() != class.parameters.len() {
            self.errors.push(TypeCheckError::new(
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

    /// The dictionary record type for a class constraint: one field per method,
    /// in declaration order, with the class parameter substituted.
    pub(in crate::typecheck) fn dictionary_type(
        &mut self,
        constraint: &ClassConstraint,
    ) -> InferType {
        let Some(class) = self.classes.get(&constraint.class_id).cloned() else {
            return record_type(Vec::new(), InferType::RowEmpty);
        };
        let fields = class
            .methods
            .iter()
            .map(|method| {
                let mut variables = HashMap::new();
                for (parameter, argument) in class.parameters.iter().zip(&constraint.arguments) {
                    variables.insert(parameter.clone(), argument.clone());
                }
                let field_ty = self.elaborate_type(&method.signature, &mut variables);
                (method.name.clone(), field_ty)
            })
            .collect();
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
        let class = self.classes.get(&class_id).cloned()?;
        let mut variables = HashMap::new();
        let mut arguments = Vec::with_capacity(class.parameters.len());
        for parameter in &class.parameters {
            let variable = self.fresh();
            variables.insert(parameter.clone(), variable.clone());
            arguments.push(variable);
        }
        let ty = self.elaborate_type(&method.signature, &mut variables);
        let constraint = ClassConstraint {
            class_id,
            arguments,
            span,
        };
        let dictionary_type = self.dictionary_type(&constraint);
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
        if scheme.variables.is_empty() {
            return (scheme.constraints.clone(), scheme.ty.clone());
        }
        let mut mapping = HashMap::new();
        for variable in &scheme.variables {
            mapping.insert(*variable, self.fresh());
        }
        let ty = substitute(&scheme.ty, &mapping);
        let constraints = scheme
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
        (constraints, ty)
    }

    pub(in crate::typecheck) fn push_wanted(
        &mut self,
        constraint: ClassConstraint,
        dictionary_type: InferType,
    ) -> usize {
        let index = self.wanted.len();
        self.wanted.push(WantedConstraint {
            class_id: constraint.class_id,
            arguments: constraint.arguments,
            dictionary_type,
            span: constraint.span,
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
        self.givens.clear();
        for (constraint, (id, _)) in constraints.iter().zip(parameters) {
            self.givens
                .push((constraint.clone(), WantedSolution::Given(*id)));
        }
    }

    pub(in crate::typecheck) fn end_givens(&mut self) {
        self.givens.clear();
    }

    /// Solves every unsolved wanted constraint against the current givens and
    /// the declared instances, reporting each unresolved constraint.
    pub(in crate::typecheck) fn solve_wanted_constraints(&mut self) {
        let wanted = std::mem::take(&mut self.wanted);
        let mut solved = Vec::with_capacity(wanted.len());
        for mut constraint in wanted {
            if constraint.solution.is_none() {
                let arguments = constraint
                    .arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument.clone()))
                    .collect::<Vec<_>>();
                constraint.solution = self.select_solution(constraint.class_id, &arguments);
                if constraint.solution.is_none() {
                    let rendered = self.display_constraint(constraint.class_id, &arguments);
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NoInstance,
                        constraint.span,
                        format!("no instance for constraint {rendered}"),
                    ));
                }
            }
            solved.push(constraint);
        }
        self.wanted = solved;
    }

    fn select_solution(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> Option<WantedSolution> {
        for (given, solution) in &self.givens {
            if given.class_id == class_id && self.constraints_match(&given.arguments, arguments) {
                return Some(solution.clone());
            }
        }
        for instance in &self.instances {
            if instance.class_id == class_id
                && self.constraints_match(&instance.head_arguments, arguments)
            {
                return Some(WantedSolution::Global(instance.symbol));
            }
        }
        None
    }

    fn constraints_match(&self, expected: &[InferType], actual: &[InferType]) -> bool {
        expected.len() == actual.len()
            && expected
                .iter()
                .zip(actual)
                .all(|(left, right)| self.infer_types_equal(left, right))
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

    fn display_constraint(&self, class_id: hir::TypeId, arguments: &[InferType]) -> String {
        let name = self
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

    /// Types an instance's method bodies against the class method signatures
    /// with the head's concrete arguments substituted, then builds its
    /// dictionary value.
    pub(in crate::typecheck) fn infer_instance_declaration(
        &mut self,
        instance: &hir::InstanceDeclaration,
    ) -> Option<InferredDeclaration> {
        let class = self.classes.get(&instance.class_id).cloned()?;
        let head_arguments = self
            .instances
            .iter()
            .find(|info| info.symbol == instance.symbol)
            .map(|info| info.head_arguments.clone())?;
        if class.superclasses != 0 || !instance.context.is_empty() {
            return None;
        }
        for member in &instance.members {
            if !class
                .methods
                .iter()
                .any(|method| method.name == member.name)
            {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    member.name_span,
                    format!(
                        "instance member `{}` is not a method of its class",
                        member.name
                    ),
                ));
                return None;
            }
        }
        let constraint = ClassConstraint {
            class_id: instance.class_id,
            arguments: head_arguments.clone(),
            span: instance.span,
        };
        let dictionary_type = self.dictionary_type(&constraint);
        self.givens.clear();
        let mut fields = Vec::with_capacity(class.methods.len());
        for method in &class.methods {
            let Some(member) = instance
                .members
                .iter()
                .find(|member| member.name == method.name)
            else {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::MissingInstanceMethod,
                    instance.span,
                    format!("instance is missing method `{}`", method.name),
                ));
                return None;
            };
            let mut variables = HashMap::new();
            for (parameter, argument) in class.parameters.iter().zip(&head_arguments) {
                variables.insert(parameter.clone(), argument.clone());
            }
            let expected = self.elaborate_type(&method.signature, &mut variables);
            let value = self.infer_expr_with_expected(&member.value, Some(expected.clone()))?;
            self.unify(expected, value.ty.clone(), member.span);
            fields.push((method.name.clone(), value));
        }
        self.solve_wanted_constraints();
        Some(InferredDeclaration {
            symbol: instance.symbol,
            name: instance.name.clone(),
            name_span: instance.name_span,
            scheme: Scheme::monomorphic(dictionary_type.clone()),
            value: InferredExpr {
                kind: InferredExprKind::Record(fields),
                ty: dictionary_type,
                span: instance.span,
            },
            span: instance.span,
        })
    }

    /// Builds the THIR evidence for a solved wanted constraint.
    pub(in crate::typecheck) fn wanted_evidence(
        &mut self,
        index: usize,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::Evidence> {
        let wanted = self.wanted.get(index)?;
        let solution = wanted.solution.clone()?;
        let class_id = wanted.class_id;
        let span = wanted.span;
        let dictionary_type = wanted.dictionary_type.clone();
        let ty = self.finalize_type(&dictionary_type, span, interner, generics)?;
        let kind = match solution {
            WantedSolution::Given(id) => thir::EvidenceKind::Given(id),
            WantedSolution::Global(symbol) => thir::EvidenceKind::Global(symbol),
        };
        Some(thir::Evidence {
            kind,
            class_id,
            ty,
            span,
        })
    }
}
