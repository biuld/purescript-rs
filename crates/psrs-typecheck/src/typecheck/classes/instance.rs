use super::super::*;
use super::UnsolvedPolicy;
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
        self.with_report_origin(instance.span, |checker| {
            checker.infer_instance_declaration_with_report_origin(instance)
        })
    }

    fn infer_instance_declaration_with_report_origin(
        &mut self,
        instance: &hir::InstanceDeclaration,
    ) -> Option<InferredDeclaration> {
        let class = self.env.classes.get(&instance.class_id).cloned()?;
        let (head_arguments, instance_variables, context, context_parameters) = {
            let info = self
                .env
                .instances
                .iter()
                .find(|info| info.symbol == instance.symbol)?;
            (
                info.head_arguments.clone(),
                info.instance_variables.clone(),
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
                self.state.errors.push(TypeCheckError::new(
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
                self.validate_known_deriving_class(
                    instance.class_id,
                    &class,
                    &instance.head,
                    &head_arguments,
                    instance.span,
                )?;
                None
            }
            Some(hir::DerivationStrategy::Newtype) => {
                if class.parameters.len() != head_arguments.len() {
                    return self.deriving_error(
                        TypeCheckErrorKind::InvalidNewtypeInstance,
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
        let wanted_start = self.state.wanted.len();
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
                    let mut method_variables = variables.clone();
                    let expected = self.elaborate_type(&method.signature, &mut method_variables);
                    let annotation_variables = instance_variables.clone();
                    let value = self.with_scope(|checker| {
                        checker.scope.annotation_variables = annotation_variables;
                        checker.in_nested_level(|checker| {
                            checker.infer_expr_with_expected(&member.value, Some(expected))
                        })
                    });
                    let Some(value) = value else {
                        self.end_givens();
                        return None;
                    };
                    Some(value)
                }
                None => match instance.derivation {
                    Some(hir::DerivationStrategy::Newtype) => self.in_nested_level(|checker| {
                        checker.derive_newtype_method(
                            instance.class_id,
                            &class,
                            method,
                            &head_arguments,
                            instance.span,
                        )
                    }),
                    Some(hir::DerivationStrategy::KnownClass) => self.in_nested_level(|checker| {
                        checker.derive_known_class_method(
                            instance.class_id,
                            &class,
                            method,
                            &head_arguments,
                            instance.span,
                        )
                    }),
                    None => {
                        self.state.errors.push(TypeCheckError::new(
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
                self.state.errors.push(TypeCheckError::new(
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
        self.solve_wanted_constraints(
            Some(&dictionary_type),
            wanted_start,
            UnsolvedPolicy::RequireSolved,
        );
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
        // them so the declaration is polymorphic in the head variables. Keep
        // variables that only occur in erased evidence too: a `Coercible`
        // superclass proof still needs them while its type is finalized.
        let mut head_variables = HashSet::new();
        for variable in instance_variables.values() {
            super::fundeps::collect_infer_variables(
                &self.resolve_type(variable.clone()),
                &mut head_variables,
            );
        }
        let head_variables = head_variables.into_iter().collect::<Vec<_>>();
        let scheme = self.generalize_instance_dictionary(&head_variables, &value.ty);
        let scheme = self.generalize_body(scheme, &value, TOP_LEVEL);
        Some(InferredDeclaration {
            symbol: instance.symbol,
            name: instance.name.clone(),
            name_span: instance.name_span,
            scheme,
            value,
            span: instance.span,
        })
    }

    /// Checks the member's type against the class method signature. The check is
    /// a trial: a member whose body types at the instance head but not at the
    /// declared signature fails the instance, and the trial leaves the solver
    /// exactly as it found it. Its diagnostic stays, because it is what the
    /// caller has to report.
    fn check_instance_method_type(
        &mut self,
        signature: &hir::Type,
        class_variables: &HashMap<String, InferType>,
        actual: &InferType,
        span: TextRange,
    ) -> bool {
        let mut class_variables = class_variables.clone();
        self.speculate_reporting(|checker| {
            let errors_before = checker.state.errors.len();
            let expected = checker.elaborate_type(signature, &mut class_variables);
            checker.unify(expected, actual.clone(), span);
            (checker.state.errors.len() == errors_before).then_some(())
        })
        .is_some()
    }
}
