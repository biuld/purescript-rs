use super::super::*;

impl Checker {
    /// Records every class in the program and its methods. Classes declared in
    /// this module are validated; imported classes are recorded so their
    /// methods can be selected and their unsupported forms rejected at use.
    pub(in crate::typecheck) fn build_class_environment(
        &mut self,
        module: &hir::Module,
        known_types: &[hir::TypeDeclaration],
    ) {
        let mut declarations = Vec::new();
        for declaration in module.types.iter().chain(known_types.iter()) {
            if declaration.kind == hir::TypeDeclarationKind::Class
                && !self.classes.contains_key(&declaration.id)
            {
                declarations.push(declaration);
            }
        }
        for declaration in declarations {
            let parameters = declaration
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let local = declaration.id.module == module.id;
            if local {
                if parameters.len() != 1 {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedClass,
                        declaration.name_span,
                        "a class must have exactly one type parameter",
                    ));
                }
                if !declaration.superclasses.is_empty() {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedClass,
                        declaration.name_span,
                        "class superclasses are not supported yet",
                    ));
                }
            }
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
                    superclasses: declaration.superclasses.len(),
                    methods,
                },
            );
        }
    }

    /// Records each instance's class, concrete head arguments, and dictionary
    /// symbol. The dictionary value itself is elaborated as a declaration.
    pub(in crate::typecheck) fn build_instance_environment(&mut self, module: &hir::Module) {
        for instance in &module.instances {
            let Some(class) = self.classes.get(&instance.class_id).cloned() else {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "an instance head names a type that is not a class",
                ));
                continue;
            };
            if class.superclasses != 0 {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "instances of classes with superclasses are not supported yet",
                ));
                continue;
            }
            let arguments = head_arguments(&instance.head);
            if arguments.len() != class.parameters.len() {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "an instance head must apply its class to one type argument per parameter",
                ));
                continue;
            }
            if !instance.context.is_empty() {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "instance contexts are not supported yet",
                ));
                continue;
            }
            let mut variables = HashMap::new();
            let head_arguments = arguments
                .iter()
                .map(|argument| self.elaborate_type(argument, &mut variables))
                .collect::<Vec<_>>();
            if !head_arguments
                .iter()
                .all(|argument| self.is_ground_type(argument))
            {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    instance.span,
                    "an instance head argument must be a concrete type",
                ));
                continue;
            }
            self.instances.push(InstanceInfo {
                symbol: instance.symbol,
                class_id: instance.class_id,
                head_arguments,
            });
        }
    }

    /// Whether a type contains no inference variables and no open row tail.
    pub(in crate::typecheck) fn is_ground_type(&self, ty: &InferType) -> bool {
        match self.resolve_type(ty.clone()) {
            InferType::Variable(_) => false,
            InferType::Application(function, argument) => {
                self.is_ground_type(&function) && self.is_ground_type(&argument)
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.is_ground_type(&ty) && self.is_ground_type(&tail)
            }
            InferType::RowEmpty | InferType::Constructor(_) => true,
        }
    }
}

/// The type arguments of an instance head such as `C T U`.
fn head_arguments(head: &hir::Type) -> Vec<&hir::Type> {
    let mut arguments = Vec::new();
    let mut expression = head;
    while let hir::TypeKind::Application(function, argument) = &expression.kind {
        arguments.push(argument.as_ref());
        expression = function;
    }
    arguments.reverse();
    arguments
}

/// Checks that a method signature mentions only its class parameters, with no
/// method-level forall or constraint.
fn validate_method_signature(signature: &hir::Type, parameters: &[String]) -> Result<(), String> {
    let mut variables = Vec::new();
    collect_signature_variables(signature, &mut variables)?;
    for variable in variables {
        if !parameters.contains(&variable) {
            return Err(format!(
                "class method signature uses `{variable}`, which is not a class parameter"
            ));
        }
    }
    Ok(())
}

fn collect_signature_variables(ty: &hir::Type, out: &mut Vec<String>) -> Result<(), String> {
    match &ty.kind {
        hir::TypeKind::Variable(name) => out.push(name.clone()),
        hir::TypeKind::Forall { .. } => {
            return Err("class method signatures with foralls are not supported yet".into());
        }
        hir::TypeKind::Constrained { .. } => {
            return Err("class method signatures with constraints are not supported yet".into());
        }
        hir::TypeKind::Application(function, argument) => {
            collect_signature_variables(function, out)?;
            collect_signature_variables(argument, out)?;
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_signature_variables(parameter, out)?;
            collect_signature_variables(result, out)?;
        }
        hir::TypeKind::Record { fields, tail } | hir::TypeKind::Row { fields, tail } => {
            for field in fields {
                collect_signature_variables(&field.ty, out)?;
            }
            if let Some(tail) = tail {
                collect_signature_variables(tail, out)?;
            }
        }
        hir::TypeKind::Constructor(_)
        | hir::TypeKind::Named(_)
        | hir::TypeKind::Opaque(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
    Ok(())
}

/// The first local ID not used by any source-local binder in the module, used
/// to synthesize dictionary parameters.
pub(in crate::typecheck) fn next_local_id(module: &hir::Module) -> u32 {
    let mut max = None;
    for declaration in &module.declarations {
        scan_expr(&declaration.value, &mut max);
    }
    for instance in &module.instances {
        for member in &instance.members {
            scan_expr(&member.value, &mut max);
        }
    }
    max.map_or(0, |value| value + 1)
}

fn note_local(id: LocalId, max: &mut Option<u32>) {
    *max = Some(max.map_or(id.0, |value| value.max(id.0)));
}

fn scan_expr(expression: &hir::Expr, max: &mut Option<u32>) {
    match &expression.kind {
        hir::ExprKind::Local(id) => note_local(*id, max),
        hir::ExprKind::Global(_)
        | hir::ExprKind::Integer(_)
        | hir::ExprKind::Number(_)
        | hir::ExprKind::String(_)
        | hir::ExprKind::Char(_) => {}
        hir::ExprKind::Array(elements) => {
            for element in elements {
                scan_expr(element, max);
            }
        }
        hir::ExprKind::Record(fields) => {
            for (_, value) in fields {
                scan_expr(value, max);
            }
        }
        hir::ExprKind::RecordUpdate { expression, fields } => {
            scan_expr(expression, max);
            for (_, value) in fields {
                scan_expr(value, max);
            }
        }
        hir::ExprKind::FieldAccess { expression, .. } => scan_expr(expression, max),
        hir::ExprKind::Application(function, argument) => {
            scan_expr(function, max);
            scan_expr(argument, max);
        }
        hir::ExprKind::Operator { left, right, .. } => {
            scan_expr(left, max);
            scan_expr(right, max);
        }
        hir::ExprKind::Lambda { binder, body } => {
            note_local(binder.id, max);
            scan_expr(body, max);
        }
        hir::ExprKind::Let { bindings, body } => {
            for binding in bindings {
                note_local(binding.binder.id, max);
                scan_expr(&binding.value, max);
            }
            scan_expr(body, max);
        }
        hir::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            scan_expr(condition, max);
            scan_expr(then_branch, max);
            scan_expr(else_branch, max);
        }
        hir::ExprKind::Case {
            scrutinee,
            branches,
        } => {
            scan_expr(scrutinee, max);
            for branch in branches {
                scan_pattern(&branch.pattern, max);
                scan_expr(&branch.value, max);
            }
        }
    }
}

fn scan_pattern(pattern: &hir::Pattern, max: &mut Option<u32>) {
    match &pattern.kind {
        hir::PatternKind::Wildcard => {}
        hir::PatternKind::Var(binder) => note_local(binder.id, max),
        hir::PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                scan_pattern(argument, max);
            }
        }
        hir::PatternKind::Record { fields } => {
            for (_, field) in fields {
                scan_pattern(field, max);
            }
        }
    }
}
