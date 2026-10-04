use super::super::signature::flatten_spine;
use super::super::*;
use super::deriving::contains_wildcard;
use super::fundeps::collect_infer_variables;
mod method;

use method::validate_method_signature;

impl Checker {
    /// Records every class in the program with its parameters, superclass
    /// edges, and methods. Classes declared in this module are validated;
    /// imported classes are recorded so their methods can be selected and
    /// their unsupported forms rejected at use.
    pub(in crate::typecheck) fn build_class_environment(
        &mut self,
        module: &hir::Module,
        known_types: &[hir::TypeDeclaration],
    ) {
        if module.imports.iter().any(|import| {
            import.module_name == "Prim.Coerce"
                || import.module_name == "Safe.Coerce"
                || import.types.iter().any(|imported| {
                    imported.reference == hir::TypeReference::Named(hir::TypeId::COERCIBLE)
                })
        }) {
            self.env
                .type_names
                .insert(hir::TypeId::COERCIBLE, "Coercible".to_owned());
            self.env.classes.insert(
                hir::TypeId::COERCIBLE,
                ClassInfo {
                    parameters: vec!["source".to_owned(), "target".to_owned()],
                    superclasses: Vec::new(),
                    fundeps: Vec::new(),
                    methods: Vec::new(),
                },
            );
        }
        let mut declarations = Vec::new();
        for declaration in module.types.iter().chain(known_types.iter()) {
            if declaration.kind == hir::TypeDeclarationKind::Class
                && !self.env.classes.contains_key(&declaration.id)
            {
                declarations.push(declaration);
            }
        }
        // Record parameters and methods for every class first so that a
        // superclass edge can refer to a class declared later in the program.
        for declaration in &declarations {
            let parameters = declaration
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let local = declaration.id.module == module.id;
            let fundeps = self.build_fundeps(declaration, &parameters, local);
            let mut methods = Vec::new();
            for member in &declaration.members {
                let Some(signature) = &member.signature else {
                    if local {
                        self.state.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedClass,
                            member.name_span,
                            "a class method requires a type signature",
                        ));
                    }
                    continue;
                };
                if local && let Err(message) = validate_method_signature(signature, &parameters) {
                    self.state.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedClass,
                        member.name_span,
                        message,
                    ));
                }
                let method = MethodInfo {
                    symbol: member.symbol,
                    name: member.name.clone(),
                    signature: signature.clone(),
                };
                self.env
                    .class_methods
                    .insert(member.symbol, (declaration.id, method.clone()));
                methods.push(method);
            }
            self.env.classes.insert(
                declaration.id,
                ClassInfo {
                    parameters,
                    superclasses: Vec::new(),
                    fundeps,
                    methods,
                },
            );
        }
        // Elaborate each class's superclass edges now that every class's
        // parameter list is known. Imported classes keep their edges so a
        // local instance of an imported class still has its superclass fields.
        for declaration in &declarations {
            let local = declaration.id.module == module.id;
            let parameters = self
                .env
                .classes
                .get(&declaration.id)
                .map(|class| class.parameters.clone())
                .unwrap_or_default();
            let mut superclasses = Vec::new();
            for (index, superclass) in declaration.superclasses.iter().enumerate() {
                if let Some(info) = self.build_superclass(superclass, &parameters, index, local) {
                    superclasses.push(info);
                }
            }
            if let Some(class) = self.env.classes.get_mut(&declaration.id) {
                class.superclasses = superclasses;
            }
        }
        self.validate_superclass_cycles(module);
    }

    /// Resolves a class's functional dependencies to parameter positions. A
    /// name that is not one of the class's type parameters is reported and the
    /// dependency is dropped so later improvement never indexes out of bounds.
    fn build_fundeps(
        &mut self,
        declaration: &hir::TypeDeclaration,
        parameters: &[String],
        local: bool,
    ) -> Vec<FundepInfo> {
        let mut fundeps = Vec::with_capacity(declaration.fundeps.len());
        for fundep in &declaration.fundeps {
            let mut determining = Vec::with_capacity(fundep.from.len());
            let mut determined = Vec::with_capacity(fundep.to.len());
            let mut valid = true;
            for (side, out) in [
                (&fundep.from, &mut determining),
                (&fundep.to, &mut determined),
            ] {
                for name in side {
                    match parameters.iter().position(|parameter| parameter == name) {
                        Some(index) => out.push(index),
                        None => {
                            if local {
                                self.state.errors.push(TypeCheckError::new(
                                    TypeCheckErrorKind::UnsupportedClass,
                                    fundep.span,
                                    "a functional dependency variable must be one of the class's type parameters",
                                ));
                            }
                            valid = false;
                        }
                    }
                }
            }
            if valid {
                fundeps.push(FundepInfo {
                    determining,
                    determined,
                });
            }
        }
        fundeps
    }

    /// Reports superclass cycles, which would make the class environment
    /// unsatisfiable and dictionary construction non-terminating.
    fn validate_superclass_cycles(&mut self, module: &hir::Module) {
        let mut status: HashMap<hir::TypeId, u8> = HashMap::new();
        let mut cycle = None;
        for declaration in &module.types {
            if declaration.kind != hir::TypeDeclarationKind::Class {
                continue;
            }
            if superclass_cycle(declaration.id, &self.env.classes, &mut status) {
                cycle = Some(declaration.name_span);
                break;
            }
        }
        if let Some(span) = cycle {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                span,
                "class superclasses form a cycle",
            ));
            // The program is rejected; drop the edges so later dictionary
            // construction does not recurse forever while diagnostics finish.
            for class in self.env.classes.values_mut() {
                class.superclasses.clear();
            }
        }
    }

    /// Records each instance's class, head arguments, elaborated context
    /// constraints, and the dictionary parameters synthesized for that
    /// context. The dictionary value and its context-solving are elaborated
    /// as a declaration.
    ///
    /// Local instances are validated; imported instances were checked in their
    /// defining module and are re-elaborated only to make them searchable.
    /// The dictionary parameters are synthesized for local instances only,
    /// because an imported instance's dictionary is an ordinary top-level value
    /// in its defining module.
    pub(in crate::typecheck) fn build_instance_environment(
        &mut self,
        module: &hir::Module,
        imported: &[hir::InstanceDeclaration],
    ) {
        for instance in &module.instances {
            self.record_instance(instance, true);
        }
        for instance in imported {
            // Re-elaboration of an imported instance must not produce
            // diagnostics or consume local dictionary parameters: its defining
            // module already validated it. The instance stays searchable, so
            // this discards diagnostics only and keeps what it elaborated.
            self.without_diagnostics(|checker| checker.record_instance(instance, false));
        }
        self.validate_instance_overlaps(module);
    }

    /// Records one instance in the searchable instance environment. `local`
    /// selects the validation and dictionary-parameter behavior: a local
    /// instance is checked and gets fresh context parameters, while an imported
    /// one is recorded as-is.
    fn record_instance(&mut self, instance: &hir::InstanceDeclaration, local: bool) {
        let Some(class) = self.env.classes.get(&instance.class_id).cloned() else {
            if local {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "an instance head names a type that is not a class",
                ));
            }
            return;
        };
        if instance.class_id == hir::TypeId::COERCIBLE {
            if local {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::InvalidCoercibleInstanceDeclaration,
                    instance.span,
                    "Coercible instances are compiler-derived and cannot be declared in source",
                ));
            }
            return;
        }
        let (_, arguments) = flatten_spine(&instance.head);
        let newtype_deriving_wildcard = local
            && instance.derivation == Some(hir::DerivationStrategy::KnownClass)
            && self
                .env
                .type_modules
                .get(&instance.class_id)
                .is_some_and(|module| module == "Data.Newtype")
            && self
                .env
                .type_names
                .get(&instance.class_id)
                .is_some_and(|name| name == "Newtype")
            && arguments.len() == 2
            && !contains_wildcard(arguments[0])
            && matches!(arguments[1].kind, hir::TypeKind::Wildcard);
        let generic_deriving_wildcard = local
            && instance.derivation == Some(hir::DerivationStrategy::KnownClass)
            && self
                .env
                .type_modules
                .get(&instance.class_id)
                .is_some_and(|module| module == "Data.Generic.Rep")
            && self
                .env
                .type_names
                .get(&instance.class_id)
                .is_some_and(|name| name == "Generic")
            && arguments.len() == 2
            && !contains_wildcard(arguments[0])
            && matches!(arguments[1].kind, hir::TypeKind::Wildcard);
        let has_unsupported_wildcard = arguments.iter().enumerate().any(|(index, argument)| {
            contains_wildcard(argument)
                && !((newtype_deriving_wildcard || generic_deriving_wildcard) && index == 1)
        });
        if local && has_unsupported_wildcard {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::InvalidInstanceHead,
                instance.head.span,
                "an instance head cannot contain a type wildcard",
            ));
            return;
        }
        if arguments.len() != class.parameters.len() {
            if local {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "an instance head must apply its class to one type argument per parameter",
                ));
            }
            return;
        }
        let mut variables = HashMap::new();
        let head_arguments = arguments
            .iter()
            .map(|argument| self.elaborate_type(argument, &mut variables))
            .collect::<Vec<_>>();
        let mut context = Vec::with_capacity(instance.context.len());
        let mut context_parameters = Vec::with_capacity(instance.context.len());
        let mut valid = true;
        for constraint in &instance.context {
            let Some(elaborated) = self.elaborate_constraint(constraint, &mut variables, true)
            else {
                valid = false;
                continue;
            };
            if local {
                let dictionary_type = self.dictionary_type(&elaborated);
                let id = LocalId(self.state.next_dictionary_local);
                self.state.next_dictionary_local += 1;
                context_parameters.push((id, dictionary_type));
            }
            context.push(elaborated);
        }
        if !valid {
            return;
        }
        let instance_variables = variables.clone();
        if local {
            let head_names = head_variables(&arguments);
            let determined =
                instance_context_determined_variables(&self.env.classes, &context, &head_arguments);
            for constraint in &instance.context {
                let mut used = Vec::new();
                collect_variables(constraint, &mut used);
                for name in used {
                    let determined_by_fundep = variables.get(&name).is_some_and(|ty| {
                        let mut variables = HashSet::new();
                        collect_infer_variables(ty, &mut variables);
                        !variables.is_empty() && variables.is_subset(&determined)
                    });
                    if !head_names.contains(&name) && !determined_by_fundep {
                        self.state.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedClass,
                            constraint.span,
                            "an instance context variable must appear in the instance head",
                        ));
                        valid = false;
                    }
                }
            }
        }
        if !valid {
            return;
        }
        self.env.instances.push(InstanceInfo {
            symbol: instance.symbol,
            class_id: instance.class_id,
            chain_id: instance.chain_id,
            chain_position: instance.chain_position,
            head_arguments,
            instance_variables,
            context,
            context_parameters,
        });
    }
}

/// The head arguments determine their own variables, and class functional
/// dependencies may determine additional variables in instance contexts. Take
/// the closure across context constraints so chained dependencies work too.
fn instance_context_determined_variables(
    classes: &HashMap<hir::TypeId, ClassInfo>,
    context: &[ClassConstraint],
    head_arguments: &[InferType],
) -> HashSet<u32> {
    let mut determined = HashSet::new();
    for argument in head_arguments {
        collect_infer_variables(argument, &mut determined);
    }
    loop {
        let mut changed = false;
        for constraint in context {
            let Some(class) = classes.get(&constraint.class_id) else {
                continue;
            };
            for fundep in &class.fundeps {
                let determining = fundep
                    .determining
                    .iter()
                    .filter_map(|&index| constraint.arguments.get(index))
                    .flat_map(|argument| {
                        let mut variables = HashSet::new();
                        collect_infer_variables(argument, &mut variables);
                        variables
                    })
                    .collect::<HashSet<_>>();
                if !determining.is_subset(&determined) {
                    continue;
                }
                for &index in &fundep.determined {
                    if let Some(argument) = constraint.arguments.get(index) {
                        let mut variables = HashSet::new();
                        collect_infer_variables(argument, &mut variables);
                        for variable in variables {
                            changed |= determined.insert(variable);
                        }
                    }
                }
            }
        }
        if !changed {
            return determined;
        }
    }
}

/// Depth-first search for a superclass cycle through `classes`.
fn superclass_cycle(
    id: hir::TypeId,
    classes: &HashMap<hir::TypeId, ClassInfo>,
    status: &mut HashMap<hir::TypeId, u8>,
) -> bool {
    match status.get(&id) {
        Some(1) => return true,
        Some(2) => return false,
        _ => {}
    }
    status.insert(id, 1);
    if let Some(class) = classes.get(&id) {
        for superclass in &class.superclasses {
            if superclass_cycle(superclass.class_id, classes, status) {
                return true;
            }
        }
    }
    status.insert(id, 2);
    false
}

/// The set of type-variable names used anywhere in a class or context type.
fn collect_variables(ty: &hir::Type, out: &mut Vec<String>) {
    match &ty.kind {
        hir::TypeKind::Variable(name) => out.push(name.clone()),
        hir::TypeKind::Application(function, argument) => {
            collect_variables(function, out);
            collect_variables(argument, out);
        }
        hir::TypeKind::OperatorChain { operands, .. } => {
            for operand in operands {
                collect_variables(operand, out);
            }
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_variables(parameter, out);
            collect_variables(result, out);
        }
        hir::TypeKind::Record { fields, tail } | hir::TypeKind::Row { fields, tail } => {
            for field in fields {
                collect_variables(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_variables(tail, out);
            }
        }
        hir::TypeKind::Forall { body, .. } => collect_variables(body, out),
        hir::TypeKind::Constrained { constraint, body } => {
            collect_variables(constraint, out);
            collect_variables(body, out);
        }
        hir::TypeKind::Wildcard
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Named(_)
        | hir::TypeKind::Opaque(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
}

/// The type-variable names appearing in an instance head's arguments.
fn head_variables(arguments: &[&hir::Type]) -> Vec<String> {
    let mut names = Vec::new();
    for argument in arguments {
        collect_variables(argument, &mut names);
    }
    names
}
