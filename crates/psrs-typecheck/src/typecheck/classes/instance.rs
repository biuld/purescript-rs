use super::super::*;

impl Checker {
    /// Types an instance's method bodies against the class method signatures
    /// with the head's arguments substituted, inserts the superclass
    /// dictionaries, and wraps the dictionary in one lambda per context
    /// dictionary.
    pub(in crate::typecheck) fn infer_instance_declaration(
        &mut self,
        instance: &hir::InstanceDeclaration,
    ) -> Option<InferredDeclaration> {
        let class = self.classes.get(&instance.class_id).cloned()?;
        let (head_arguments, context, context_parameters) = {
            let info = self
                .instances
                .iter()
                .find(|info| info.symbol == instance.symbol)?;
            (
                info.head_arguments.clone(),
                info.context.clone(),
                info.context_parameters.clone(),
            )
        };
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
        self.begin_givens(&context, &context_parameters);

        let mut fields = Vec::with_capacity(class.superclasses.len() + class.methods.len());
        for superclass in &class.superclasses {
            let mut arguments = Vec::with_capacity(superclass.arguments.len());
            for name in &superclass.arguments {
                let index = class
                    .parameters
                    .iter()
                    .position(|parameter| parameter == name)?;
                arguments.push(head_arguments[index].clone());
            }
            let super_constraint = ClassConstraint {
                class_id: superclass.class_id,
                arguments,
                span: superclass.span,
            };
            let super_dictionary = self.dictionary_type(&super_constraint);
            let wanted = self.push_wanted(super_constraint, super_dictionary.clone());
            fields.push((
                superclass.field.clone(),
                InferredExpr {
                    kind: InferredExprKind::Evidence(wanted),
                    ty: super_dictionary,
                    span: instance.span,
                },
            ));
        }
        let mut variables = HashMap::new();
        for (parameter, argument) in class.parameters.iter().zip(&head_arguments) {
            variables.insert(parameter.clone(), argument.clone());
        }
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
                self.end_givens();
                return None;
            };
            let expected = self.elaborate_type(&method.signature, &mut variables);
            let Some(value) = self.infer_expr_with_expected(&member.value, Some(expected.clone()))
            else {
                self.end_givens();
                return None;
            };
            self.unify(expected, value.ty.clone(), member.span);
            fields.push((method.name.clone(), value));
        }
        self.solve_wanted_constraints();
        self.end_givens();
        let value = self.wrap_dictionary_lambdas(
            InferredExpr {
                kind: InferredExprKind::Record(fields),
                ty: dictionary_type,
                span: instance.span,
            },
            &context_parameters,
        );
        // An instance head may contain type variables (for example a
        // `ToInt (Array a)` head); generalize the dictionary constructor over
        // them so the declaration is polymorphic in the head variables.
        let scheme = self.generalize(&value.ty, &[], TOP_LEVEL);
        Some(InferredDeclaration {
            symbol: instance.symbol,
            name: instance.name.clone(),
            name_span: instance.name_span,
            scheme,
            value,
            span: instance.span,
        })
    }
}
