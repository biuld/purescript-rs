use super::super::signature::{flatten_spine, nominal_type_id};
use super::super::*;
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
            self.type_names
                .insert(hir::TypeId::COERCIBLE, "Coercible".to_owned());
            self.classes.insert(
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
                && !self.classes.contains_key(&declaration.id)
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
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedClass,
                            member.name_span,
                            "a class method requires a type signature",
                        ));
                    }
                    continue;
                };
                if local && let Err(message) = validate_method_signature(signature, &parameters) {
                    self.errors.push(TypeCheckError::new(
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
                self.class_methods
                    .insert(member.symbol, (declaration.id, method.clone()));
                methods.push(method);
            }
            self.classes.insert(
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
            if let Some(class) = self.classes.get_mut(&declaration.id) {
                class.superclasses = superclasses;
            }
        }
        self.validate_superclass_cycles(module);
    }

    /// Elaborates one superclass edge `C τ...`. Every argument must be one of
    /// the subclass's type parameters; the edge's field name follows the
    /// official compiler's `ClassName<index>` scheme. `local` gates the
    /// diagnostics because an imported class was already checked in its module.
    fn build_superclass(
        &mut self,
        superclass: &hir::Type,
        parameters: &[String],
        index: usize,
        local: bool,
    ) -> Option<SuperclassInfo> {
        let (head, arguments) = flatten_spine(superclass);
        let Some(class_id) = nominal_type_id(head) else {
            if local {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    superclass.span,
                    "a superclass must name a class",
                ));
            }
            return None;
        };
        let Some(arity) = self
            .classes
            .get(&class_id)
            .map(|class| class.parameters.len())
        else {
            if local {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    superclass.span,
                    "a superclass names an unknown class",
                ));
            }
            return None;
        };
        if arguments.len() != arity {
            if local {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    superclass.span,
                    "a superclass constraint has the wrong number of type arguments",
                ));
            }
            return None;
        }
        let mut names = Vec::with_capacity(arguments.len());
        for argument in arguments {
            match &argument.kind {
                hir::TypeKind::Variable(name) if parameters.contains(name) => {
                    names.push(name.clone());
                }
                _ => {
                    if local {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedClass,
                            argument.span,
                            "a superclass argument must be one of the class's type parameters",
                        ));
                    }
                    return None;
                }
            }
        }
        let name = self
            .type_names
            .get(&class_id)
            .cloned()
            .unwrap_or_else(|| format!("Class{}", class_id.index));
        Some(SuperclassInfo {
            class_id,
            arguments: names,
            field: format!("{name}{index}"),
            span: superclass.span,
        })
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
                                self.errors.push(TypeCheckError::new(
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
            if superclass_cycle(declaration.id, &self.classes, &mut status) {
                cycle = Some(declaration.name_span);
                break;
            }
        }
        if let Some(span) = cycle {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedClass,
                span,
                "class superclasses form a cycle",
            ));
            // The program is rejected; drop the edges so later dictionary
            // construction does not recurse forever while diagnostics finish.
            for class in self.classes.values_mut() {
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
            // module already validated it. Any error here is dropped so an
            // unsupported form there does not surface as an error here.
            let errors_before = self.errors.len();
            self.record_instance(instance, false);
            self.errors.truncate(errors_before);
        }
        self.validate_instance_overlaps(module);
    }

    /// Records one instance in the searchable instance environment. `local`
    /// selects the validation and dictionary-parameter behavior: a local
    /// instance is checked and gets fresh context parameters, while an imported
    /// one is recorded as-is.
    fn record_instance(&mut self, instance: &hir::InstanceDeclaration, local: bool) {
        let Some(class) = self.classes.get(&instance.class_id).cloned() else {
            if local {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "an instance head names a type that is not a class",
                ));
            }
            return;
        };
        if instance.class_id == hir::TypeId::COERCIBLE {
            if local {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::InvalidCoercibleInstanceDeclaration,
                    instance.span,
                    "Coercible instances are compiler-derived and cannot be declared in source",
                ));
            }
            return;
        }
        let (_, arguments) = flatten_spine(&instance.head);
        if arguments.len() != class.parameters.len() {
            if local {
                self.errors.push(TypeCheckError::new(
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
        let head_variable_types = variables.clone();
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
                let id = LocalId(self.next_dictionary_local);
                self.next_dictionary_local += 1;
                context_parameters.push((id, dictionary_type));
            }
            context.push(elaborated);
        }
        if !valid {
            return;
        }
        if local {
            let head_names = head_variables(&arguments);
            for constraint in &instance.context {
                let mut used = Vec::new();
                collect_variables(constraint, &mut used);
                for name in used {
                    if !head_names.contains(&name) {
                        self.errors.push(TypeCheckError::new(
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
        self.instances.push(InstanceInfo {
            symbol: instance.symbol,
            class_id: instance.class_id,
            chain_id: instance.chain_id,
            chain_position: instance.chain_position,
            head_arguments,
            head_variables: head_variable_types,
            context,
            context_parameters,
        });
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
