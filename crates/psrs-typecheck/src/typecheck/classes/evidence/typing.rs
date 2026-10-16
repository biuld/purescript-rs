use super::super::super::signature::{flatten_spine, nominal_type_id};
use super::super::super::unify::substitute;
use super::super::super::*;
use super::record_field_type;

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
    /// superclass (holding a Unit thunk for that superclass's dictionary) followed by one field
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
            fields.push((
                field,
                arrow(InferType::Constructor(TypeConstructor::Unit), field_ty),
            ));
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
        let constraints = constraints
            .into_iter()
            .map(|mut constraint| {
                // An obligation becomes wanted when this expression is used.
                // Imported schemes keep their declaration spans for source
                // tooling, but diagnostics and warnings belong to the use site.
                constraint.span = span;
                constraint
            })
            .collect::<Vec<_>>();
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

    /// Uses an inferred expression at a scheme, instantiating its quantified
    /// variables and applying the dictionaries its constrained type exposes.
    /// This is shared by variables, applications, and contextual ascriptions so
    /// none can silently discard a constrained expression's evidence.
    pub(in crate::typecheck) fn instantiate_expression_use(
        &mut self,
        mut expression: InferredExpr,
        scheme: &Scheme,
        span: TextRange,
    ) -> InferredExpr {
        let (constraints, ty) = self.instantiate_use(scheme);
        expression.ty = ty;
        expression.span = span;
        self.apply_constraints(expression, constraints, span)
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
        let id = self.fresh_wanted_id();
        self.state.wanted.push(WantedConstraint {
            id,
            class_id: constraint.class_id,
            arguments: constraint.arguments,
            dictionary_type,
            span: constraint.span,
            report_span: self.scope.report_origin.unwrap_or(constraint.span),
            givens: self.scope.givens.clone(),
            solution: None,
        });
        index
    }
}
