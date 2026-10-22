use super::*;

impl Checker {
    pub(super) fn validate_compiler_class(
        &mut self,
        declaration: &hir::TypeDeclaration,
    ) -> Option<hir::CompilerClass> {
        let identity = declaration.compiler_class?;
        let valid = match identity {
            hir::CompilerClass::IsSymbol => self.valid_symbol_interface(declaration, identity),
            hir::CompilerClass::Reflectable => self.valid_reflectable_interface(declaration),
        };
        if valid {
            Some(identity)
        } else {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                declaration.span,
                "compiler class declaration does not match its dictionary contract",
            ));
            None
        }
    }

    fn valid_reflectable_interface(&mut self, declaration: &hir::TypeDeclaration) -> bool {
        if declaration.parameters.len() != 2
            || !declaration.superclasses.is_empty()
            || declaration.members.len() != 1
            || declaration.members[0].name != hir::CompilerClass::Reflectable.method()
            || declaration.fundeps.len() != 1
            || declaration.fundeps[0].from != [declaration.parameters[0].name.clone()]
            || declaration.fundeps[0].to != [declaration.parameters[1].name.clone()]
        {
            return false;
        }
        let Some(kind) = self.env.checked_kinds.kind_scheme(declaration.id).cloned() else {
            return false;
        };
        if !matches!(
            &kind.kind,
            Kind::Function(parameter, _)
                if matches!(parameter.as_ref(), Kind::Variable(id) if kind.variables == [*id])
        ) {
            return false;
        }
        let Kind::Function(parameter_kind, rest) = self.instantiate_kind_scheme(&kind) else {
            return false;
        };
        if *rest
            != Kind::Function(
                Box::new(Kind::Builtin(hir::BuiltinType::Type)),
                Box::new(Kind::Builtin(hir::BuiltinType::Constraint)),
            )
        {
            return false;
        }
        let Some(signature) = declaration.members[0].signature.as_ref() else {
            return false;
        };
        let argument = self.fresh();
        let result = self.fresh();
        let (InferType::Variable(argument_id), InferType::Variable(result_id)) =
            (&argument, &result)
        else {
            unreachable!()
        };
        self.record_variable_kind(*argument_id, *parameter_kind);
        self.record_variable_kind(*result_id, Kind::Builtin(hir::BuiltinType::Type));
        self.with_skolem_scope(&[*argument_id, *result_id], |checker| {
            let mut variables = HashMap::from([
                (declaration.parameters[0].name.clone(), argument.clone()),
                (declaration.parameters[1].name.clone(), result.clone()),
            ]);
            let ty = checker.elaborate_type_mode(signature, &mut variables, true);
            let InferType::Application(function, returned) = checker.resolve_type(ty) else {
                return false;
            };
            let Some((InferType::Application(proxy, parameter), _)) =
                infer_arrow_parts(&function, &returned)
            else {
                return false;
            };
            matches!(*proxy, InferType::Constructor(TypeConstructor::User(_)))
                && *parameter == argument
                && *returned == result
        })
    }

    fn valid_symbol_interface(
        &mut self,
        declaration: &hir::TypeDeclaration,
        identity: hir::CompilerClass,
    ) -> bool {
        if declaration.parameters.len() != 1
            || !declaration.superclasses.is_empty()
            || !declaration.fundeps.is_empty()
            || declaration.members.len() != 1
            || declaration.members[0].name != identity.method()
        {
            return false;
        }
        let Some(kind) = self.env.checked_kinds.kind_scheme(declaration.id).cloned() else {
            return false;
        };
        let Kind::Function(parameter_kind, _) = self.instantiate_kind_scheme(&kind) else {
            return false;
        };
        if *parameter_kind != Kind::Builtin(hir::BuiltinType::Symbol) {
            return false;
        }
        let Some(signature) = declaration.members[0].signature.as_ref() else {
            return false;
        };
        let argument = self.fresh();
        let InferType::Variable(id) = argument else {
            unreachable!()
        };
        self.record_variable_kind(id, *parameter_kind);
        self.with_skolem_scope(&[id], |checker| {
            let mut variables =
                HashMap::from([(declaration.parameters[0].name.clone(), argument.clone())]);
            let ty = checker.elaborate_type_mode(signature, &mut variables, true);
            let ty = checker.resolve_type(ty);
            // Validate the elaborated contract, so synonyms and imported aliases
            // receive exactly the same treatment as the written arrow.
            let InferType::Application(function, result) = ty else {
                return false;
            };
            let InferType::Application(head, parameter) = *function else {
                return false;
            };
            let InferType::Application(proxy, parameter_argument) = *parameter else {
                return false;
            };
            *head == InferType::Constructor(TypeConstructor::Function)
                && *result == InferType::Constructor(TypeConstructor::String)
                && matches!(*proxy, InferType::Constructor(TypeConstructor::User(_)))
                && *parameter_argument == argument
        })
    }
}
