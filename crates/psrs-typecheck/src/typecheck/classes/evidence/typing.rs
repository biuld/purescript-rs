use super::super::super::signature::{flatten_spine, nominal_type_id};
use super::super::super::*;

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

    /// Types a class method at a use site.
    ///
    /// Class parameters are the visible quantifier prefix. The class constraint
    /// stays under that quantifier until a visible application instantiates it,
    /// so `method @Int` selects the class parameter instead of an already-solved
    /// unknown.
    pub(in crate::typecheck) fn infer_method_use(
        &mut self,
        class_id: hir::TypeId,
        method: MethodInfo,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let class = self.env.classes.get(&class_id).cloned()?;
        let mut names = HashMap::new();
        let mut class_variables = Vec::with_capacity(class.parameters.len());
        let mut arguments = Vec::with_capacity(class.parameters.len());
        for parameter in &class.parameters {
            let InferType::Variable(id) = self.fresh() else {
                unreachable!("fresh inference types are variables")
            };
            self.state.rigid.insert(id);
            self.scope.type_variable_names.insert(id, parameter.clone());
            names.insert(parameter.clone(), InferType::Variable(id));
            class_variables.push(id);
            arguments.push(InferType::Variable(id));
        }
        let elaborated = self.elaborate_type_mode(&method.signature, &mut names, false);
        // The method body forces kinds onto `a` and `b` before the class
        // parameter is instantiated, so `f a` cannot later be used at a
        // different kind from `(a -> b)`.
        self.note_elaborated_kinds(&[], &elaborated, span);
        let (method_variables, method_visible, method_constraints, body) =
            Self::split_quantifier_prefix(elaborated);
        for id in &method_variables {
            self.state.rigid.insert(*id);
        }
        let mut variables = class_variables;
        let mut visible = vec![true; variables.len()];
        variables.extend(method_variables);
        visible.extend(method_visible);
        let mut constraints = vec![ClassConstraint {
            class_id,
            arguments,
            span,
        }];
        constraints.extend(method_constraints);
        let ty = quantify_method(variables, visible, constraints, body);
        Some((
            InferredExprKind::PendingMethod {
                method: method.name,
                class_id,
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
        // A leading class constraint on a method selects that method's
        // dictionary field. It is not a function argument. Every other
        // constraint is a dictionary lambda on the declaration, so the inner
        // expression keeps the whole arrow spine and each application peels one.
        let mut expression = InferredExpr { span, ..base };
        let mut dictionaries = Vec::new();
        for mut constraint in constraints {
            constraint.span = span;
            if dictionaries.is_empty()
                && let InferredExprKind::PendingMethod { method, class_id } = &expression.kind
                && *class_id == constraint.class_id
            {
                let dictionary_type = self.dictionary_type(&constraint);
                let wanted = self.push_wanted(constraint, dictionary_type);
                let method = method.clone();
                expression = InferredExpr {
                    kind: InferredExprKind::Method { method, wanted },
                    ty: expression.ty,
                    span,
                };
            } else {
                dictionaries.push(constraint);
            }
        }
        if dictionaries.is_empty() {
            return expression;
        }
        let mut current = expression.ty.clone();
        let mut shapes = vec![current.clone()];
        for constraint in dictionaries.iter().rev() {
            let dictionary_type = self.dictionary_type(constraint);
            current = arrow(dictionary_type, current);
            shapes.push(current.clone());
        }
        shapes.reverse();
        expression.ty = shapes[0].clone();
        for (index, constraint) in dictionaries.into_iter().enumerate() {
            let dictionary_type = self.dictionary_type(&constraint);
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
            witness: None,
        });
        index
    }
}

fn quantify_method(
    variables: Vec<u32>,
    visible: Vec<bool>,
    constraints: Vec<ClassConstraint>,
    body: InferType,
) -> InferType {
    let body = if constraints.is_empty() {
        body
    } else {
        InferType::Constrained {
            constraints,
            body: Box::new(body),
        }
    };
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
