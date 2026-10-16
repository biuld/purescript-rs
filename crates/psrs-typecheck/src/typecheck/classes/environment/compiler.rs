use super::*;

impl Checker {
    pub(super) fn validate_compiler_class(
        &mut self,
        declaration: &hir::TypeDeclaration,
    ) -> Option<hir::CompilerClass> {
        let identity = declaration.compiler_class?;
        let valid = match identity {
            hir::CompilerClass::IsSymbol => self.valid_symbol_interface(declaration, identity),
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
