use super::super::*;
use super::evidence::record_field_type;

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
        let (head_arguments, head_variables, context, context_parameters) = {
            let info = self
                .instances
                .iter()
                .find(|info| info.symbol == instance.symbol)?;
            (
                info.head_arguments.clone(),
                info.head_variables.clone(),
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
        let derived_newtype_underlying = match instance.derivation {
            Some(hir::DerivationStrategy::KnownClass) => {
                self.validate_known_deriving_class(instance.class_id, &class, instance.span)?;
                None
            }
            Some(hir::DerivationStrategy::Newtype) => {
                if class.parameters.len() != head_arguments.len() {
                    return self.deriving_error(
                        instance.span,
                        "derive newtype class head has the wrong arity",
                    );
                }
                Some(self.validate_newtype_deriving_instance(&head_arguments, instance.span)?)
            }
            None => None,
        };
        let constraint = ClassConstraint {
            class_id: instance.class_id,
            arguments: head_arguments.clone(),
            span: instance.span,
        };
        let dictionary_type = self.dictionary_type(&constraint);
        let wanted_start = self.wanted.len();
        self.begin_givens(&context, &context_parameters);

        if let Some(underlying) = derived_newtype_underlying {
            let mut underlying_arguments = head_arguments.clone();
            *underlying_arguments.last_mut()? = underlying;
            let underlying_constraint = ClassConstraint {
                class_id: instance.class_id,
                arguments: underlying_arguments,
                span: instance.span,
            };
            let underlying_dictionary_type = self.dictionary_type(&underlying_constraint);
            self.push_wanted(underlying_constraint, underlying_dictionary_type);
        }

        let mut fields = Vec::with_capacity(class.superclasses.len() + class.methods.len());
        for (field, super_constraint) in
            self.superclass_constraints(instance.class_id, &head_arguments)
        {
            let super_dictionary = self.dictionary_type(&super_constraint);
            let wanted = self.push_wanted(super_constraint, super_dictionary.clone());
            fields.push((
                field,
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
            let value = match instance
                .members
                .iter()
                .find(|member| member.name == method.name)
            {
                Some(member) => {
                    let outer_level = self.level;
                    self.level = outer_level + 1;
                    let mut method_variables = variables.clone();
                    let expected = self.elaborate_type(&method.signature, &mut method_variables);
                    let previous_annotation_variables =
                        std::mem::replace(&mut self.annotation_variables, head_variables.clone());
                    let value = self.infer_expr_with_expected(&member.value, Some(expected));
                    self.annotation_variables = previous_annotation_variables;
                    self.level = outer_level;
                    let Some(value) = value else {
                        self.end_givens();
                        return None;
                    };
                    Some(value)
                }
                None => match instance.derivation {
                    Some(hir::DerivationStrategy::Newtype) => {
                        let outer_level = self.level;
                        self.level = outer_level + 1;
                        let value = self.derive_newtype_method(
                            instance.class_id,
                            &class,
                            method,
                            &head_arguments,
                            instance.span,
                        );
                        self.level = outer_level;
                        value
                    }
                    Some(hir::DerivationStrategy::KnownClass) => {
                        let outer_level = self.level;
                        self.level = outer_level + 1;
                        let value = self.derive_known_class_method(
                            instance.class_id,
                            &class,
                            method,
                            &head_arguments,
                            instance.span,
                        );
                        self.level = outer_level;
                        value
                    }
                    None => {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::MissingInstanceMethod,
                            instance.span,
                            format!("instance is missing method `{}`", method.name),
                        ));
                        None
                    }
                },
            };
            let Some(value) = value else {
                self.end_givens();
                return None;
            };
            if !self.check_instance_method_type(
                &method.signature,
                &variables,
                &value.ty,
                instance.span,
            ) {
                self.end_givens();
                return None;
            }
            let Some(dictionary_method_type) = record_field_type(&dictionary_type, &method.name)
            else {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    instance.span,
                    format!("dictionary has no field for method `{}`", method.name),
                ));
                self.end_givens();
                return None;
            };
            self.unify(dictionary_method_type, value.ty.clone(), instance.span);
            fields.push((method.name.clone(), value));
        }
        self.solve_wanted_constraints(Some(&dictionary_type), wanted_start);
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

    fn check_instance_method_type(
        &mut self,
        signature: &hir::Type,
        class_variables: &HashMap<String, InferType>,
        actual: &InferType,
        span: TextRange,
    ) -> bool {
        let substitutions = self.substitutions.clone();
        let levels = self.levels.clone();
        let rigid = self.rigid.clone();
        let error_count = self.errors.len();
        let mut method_variables = class_variables.clone();
        let expected = self.elaborate_type(signature, &mut method_variables);
        self.unify(expected, actual.clone(), span);
        let valid = self.errors.len() == error_count;
        self.substitutions = substitutions;
        self.levels = levels;
        self.rigid = rigid;
        valid
    }
}
