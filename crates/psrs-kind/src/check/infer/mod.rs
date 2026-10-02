use super::*;
use crate::kind::{builtin_type_kind, flatten_spine, occurs, substitute};
use psrs_hir::BuiltinType;

mod pattern_annotations;
mod type_kind;

impl Checker<'_> {
    pub(super) fn checked_schemes(&self) -> HashMap<TypeId, KindScheme> {
        self.schemes
            .iter()
            .map(|(id, scheme)| {
                let kind = self.resolve(scheme.kind.clone());
                let mut variables = scheme.variables.clone();
                let mut free = Vec::new();
                collect_kind_variables(&kind, &mut free);
                free.sort_unstable();
                free.dedup();
                for variable in free {
                    if !variables.contains(&variable) {
                        variables.push(variable);
                    }
                }
                (*id, KindScheme { variables, kind })
            })
            .collect()
    }

    // ----- Unification -------------------------------------------------

    fn resolve(&self, kind: Kind) -> Kind {
        match kind {
            Kind::Variable(variable) => self
                .substitutions
                .get(&variable)
                .cloned()
                .map(|kind| self.resolve(kind))
                .unwrap_or(Kind::Variable(variable)),
            Kind::App(function, argument) => Kind::App(
                Box::new(self.resolve(*function)),
                Box::new(self.resolve(*argument)),
            ),
            Kind::Function(parameter, result) => Kind::Function(
                Box::new(self.resolve(*parameter)),
                Box::new(self.resolve(*result)),
            ),
            primitive => primitive,
        }
    }

    fn unify(&mut self, left: Kind, right: Kind, span: TextRange) {
        let left = self.resolve(left);
        let right = self.resolve(right);
        match (left, right) {
            (Kind::Variable(a), Kind::Variable(b)) if a == b => {}
            (Kind::Variable(a), Kind::Variable(b)) => {
                match (self.rigid.contains(&a), self.rigid.contains(&b)) {
                    (true, true) => self.kinds_do_not_unify(span),
                    (true, false) => self.bind(b, Kind::Variable(a), span),
                    (false, _) => self.bind(a, Kind::Variable(b), span),
                }
            }
            (Kind::Variable(a), _) if self.rigid.contains(&a) => self.kinds_do_not_unify(span),
            (_, Kind::Variable(b)) if self.rigid.contains(&b) => self.kinds_do_not_unify(span),
            (Kind::Variable(a), other) | (other, Kind::Variable(a)) => self.bind(a, other, span),
            (Kind::Type, Kind::Type)
            | (Kind::Constraint, Kind::Constraint)
            | (Kind::Symbol, Kind::Symbol)
            | (Kind::Row, Kind::Row) => {}
            (Kind::Builtin(a), Kind::Builtin(b)) if a == b => {}
            (Kind::Named(a), Kind::Named(b)) if a == b => {}
            (Kind::App(f1, a1), Kind::App(f2, a2)) => {
                self.unify(*f1, *f2, span);
                self.unify(*a1, *a2, span);
            }
            (Kind::Function(p1, r1), Kind::Function(p2, r2)) => {
                self.unify(*p1, *p2, span);
                self.unify(*r1, *r2, span);
            }
            _ => self.kinds_do_not_unify(span),
        }
    }

    fn bind(&mut self, variable: u32, kind: Kind, span: TextRange) {
        if occurs(variable, &kind) {
            self.report(
                INFINITE_KIND,
                span,
                "the kind of this declaration is infinite",
            );
        } else {
            self.substitutions.insert(variable, kind);
        }
    }

    fn kinds_do_not_unify(&mut self, span: TextRange) {
        self.report(KINDS_DO_NOT_UNIFY, span, "kinds do not unify");
    }

    // ----- Scheme construction ----------------------------------------

    fn instantiate_named(&mut self, id: TypeId) -> Kind {
        match self.schemes.get(&id).cloned() {
            Some(scheme) if !scheme.variables.is_empty() => {
                let mut mapping = HashMap::new();
                for variable in scheme.variables {
                    mapping.insert(variable, self.fresh());
                }
                substitute(&scheme.kind, &mapping)
            }
            Some(scheme) => scheme.kind,
            None => {
                // An imported type whose kind is not available. Give it one
                // fresh variable so repeated uses agree with each other.
                let kind = self.fresh();
                self.schemes
                    .insert(id, KindScheme::monomorphic(kind.clone()));
                kind
            }
        }
    }

    pub(super) fn build_schemes(&mut self) {
        let primitives = psrs_hir::primitive_type_declarations();
        for declaration in self
            .module
            .types
            .iter()
            .chain(primitives.iter().map(|(_, declaration)| declaration))
        {
            let scheme = match &declaration.declared_kind {
                Some(signature) => {
                    let (variables, kind) = self.parse_declared_kind(signature);
                    KindScheme { variables, kind }
                }
                None => {
                    let mut scope = HashMap::new();
                    let mut parameters = Vec::new();
                    for parameter in &declaration.parameters {
                        let kind = match &parameter.kind {
                            Some(annotation) => self.denote_kind(annotation, &mut scope),
                            None => self.fresh(),
                        };
                        scope.insert(parameter.name.clone(), kind.clone());
                        parameters.push(kind);
                    }
                    let result = match declaration.kind {
                        TypeDeclarationKind::Data | TypeDeclarationKind::Newtype => Kind::Type,
                        TypeDeclarationKind::Class => Kind::Constraint,
                        TypeDeclarationKind::TypeSynonym => self.fresh(),
                        // A foreign data declaration always carries its kind.
                        // This arm is only the fallback when that kind is absent.
                        TypeDeclarationKind::Foreign => Kind::Type,
                    };
                    let kind = parameters
                        .into_iter()
                        .rev()
                        .fold(result, |result, parameter| {
                            Kind::Function(Box::new(parameter), Box::new(result))
                        });
                    KindScheme::monomorphic(kind)
                }
            };
            self.schemes.insert(declaration.id, scheme);
        }
    }

    fn parse_declared_kind(&mut self, signature: &hir::Type) -> (Vec<u32>, Kind) {
        let mut variables = Vec::new();
        let mut scope = HashMap::new();
        let mut body = signature;
        while let TypeKind::Forall {
            variables: binders,
            body: inner,
        } = &body.kind
        {
            for binder in binders {
                if let Some(annotation) = &binder.kind {
                    let kind = self.denote_kind(annotation, &mut scope);
                    scope.insert(binder.name.clone(), kind);
                }
                let variable = self.fresh();
                self.rigid.insert(match variable {
                    Kind::Variable(id) => id,
                    _ => unreachable!("fresh kinds are variables"),
                });
                scope.insert(binder.name.clone(), variable.clone());
                if let Kind::Variable(id) = variable {
                    variables.push(id);
                }
            }
            body = inner;
        }
        let kind = self.denote_kind(body, &mut scope);
        (variables, kind)
    }

    // ----- Kind denotation (annotations) ------------------------------

    fn denote_kind(&mut self, ty: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        match &ty.kind {
            TypeKind::Wildcard => self.fresh(),
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| self.fresh()),
            TypeKind::Constructor(builtin) => match builtin {
                BuiltinType::Type => Kind::Type,
                BuiltinType::Constraint => Kind::Constraint,
                BuiltinType::Symbol => Kind::Symbol,
                BuiltinType::Row => Kind::Row,
                other => Kind::Builtin(*other),
            },
            TypeKind::Named(id) | TypeKind::Opaque(id) => Kind::Named(*id),
            TypeKind::Application(..) => {
                let (head, arguments) = flatten_spine(ty);
                self.check_partial_synonym(head, arguments.len(), ty.span);
                let mut kind = self.denote_kind(head, scope);
                for argument in arguments {
                    let argument = self.denote_kind(argument, scope);
                    kind = Kind::App(Box::new(kind), Box::new(argument));
                }
                kind
            }
            TypeKind::OperatorChain { .. } => {
                self.report(
                    "UnloweredTypeOperator",
                    ty.span,
                    "type operator chain reached kind denotation before P4",
                );
                self.fresh()
            }
            TypeKind::Function { parameter, result } => Kind::Function(
                Box::new(self.denote_kind(parameter, scope)),
                Box::new(self.denote_kind(result, scope)),
            ),
            TypeKind::Forall { variables, body } => {
                let saved = scope.clone();
                for variable in variables {
                    if let Some(annotation) = &variable.kind {
                        let kind = self.denote_kind(annotation, scope);
                        scope.insert(variable.name.clone(), kind);
                    } else {
                        scope.insert(variable.name.clone(), Kind::Type);
                    }
                }
                let kind = self.denote_kind(body, scope);
                *scope = saved;
                kind
            }
            TypeKind::Constrained { body, .. } => self.denote_kind(body, scope),
            TypeKind::Row { .. } => Kind::App(Box::new(Kind::Row), Box::new(Kind::Type)),
            TypeKind::Record { .. } => Kind::Type,
            TypeKind::Integer(_) => Kind::Builtin(BuiltinType::Int),
            TypeKind::String(_) => Kind::Symbol,
        }
    }

    // ----- Definition checking -----------------------------------------

    pub(super) fn check_definitions(&mut self) {
        for declaration in &self.module.declarations {
            if let Some(signature) = &declaration.signature {
                let mut scope = HashMap::new();
                let kind = self.kind_of_type(signature, &mut scope);
                self.unify(kind, Kind::Type, signature.span);
            }
        }
        for declaration in &self.module.types {
            let Some(scheme) = self.schemes.get(&declaration.id).cloned() else {
                continue;
            };
            let (parameters, result) = strip_function(&scheme.kind, declaration.parameters.len());
            let mut scope = HashMap::new();
            for (parameter, kind) in declaration.parameters.iter().zip(parameters) {
                scope.insert(parameter.name.clone(), kind);
            }
            match declaration.kind {
                TypeDeclarationKind::Data | TypeDeclarationKind::Newtype => {
                    for constructor in &declaration.constructors {
                        for field in &constructor.fields {
                            let kind = self.kind_of_type(field, &mut scope);
                            self.unify(kind, Kind::Type, field.span);
                        }
                    }
                }
                TypeDeclarationKind::TypeSynonym => {
                    if let Some(body) = &declaration.body {
                        let kind = self.kind_of_type(body, &mut scope);
                        self.unify(kind, result, body.span);
                    }
                }
                TypeDeclarationKind::Class => {
                    for superclass in &declaration.superclasses {
                        let kind = self.kind_of_type(superclass, &mut scope);
                        self.unify(kind, Kind::Constraint, superclass.span);
                    }
                    for member in &declaration.members {
                        if let Some(signature) = &member.signature {
                            let kind = self.kind_of_type(signature, &mut scope);
                            self.unify(kind, Kind::Type, signature.span);
                        }
                    }
                }
                TypeDeclarationKind::Foreign => {}
            }
        }
        self.check_instance_heads();
        self.check_local_type_annotations();
    }

    /// Checks every instance head against its class's kind scheme.
    ///
    /// An instance head is the class applied to the instance arguments, so its
    /// kind has to be a constraint. The class scheme is also what says at which
    /// kinds those arguments are allowed, and a standalone kind signature on the
    /// class is the only way to say so: a class without one is inferred as its
    /// parameters at `Type` returning `Constraint`, which accepts any argument
    /// kind and therefore cannot reject a head. Without this an instance is the
    /// one place a type is applied to a kind signature and never checked, so
    /// `class C :: Constraint -> Constraint` accepts `instance C Int`.
    fn check_instance_heads(&mut self) {
        for instance in &self.module.instances {
            // A head that applies its class to the wrong number of arguments is
            // an arity error, not a kind error, and the class environment owns
            // that rule. Checking it here too would report `KindsDoNotUnify`
            // where `ClassInstanceArityMismatch` is the official code, and would
            // pre-empt the arity diagnostic entirely. A class this module does
            // not declare has no known arity here either, so it is left alone.
            let Some(declared) = self
                .module
                .types
                .iter()
                .find(|declaration| declaration.id == instance.class_id)
            else {
                continue;
            };
            let (_, arguments) = flatten_spine(&instance.head);
            if arguments.len() != declared.parameters.len() {
                continue;
            }
            let mut scope = HashMap::new();
            let kind = self.kind_of_type_against(&instance.head, &mut scope, false);
            self.unify(kind, Kind::Constraint, instance.head.span);
        }
    }
}

fn collect_kind_variables(kind: &Kind, out: &mut Vec<u32>) {
    match kind {
        Kind::Variable(variable) => out.push(*variable),
        Kind::App(function, argument) => {
            collect_kind_variables(function, out);
            collect_kind_variables(argument, out);
        }
        Kind::Function(parameter, result) => {
            collect_kind_variables(parameter, out);
            collect_kind_variables(result, out);
        }
        Kind::Type
        | Kind::Constraint
        | Kind::Symbol
        | Kind::Row
        | Kind::Builtin(_)
        | Kind::Named(_) => {}
    }
}

fn strip_function(kind: &Kind, arguments: usize) -> (Vec<Kind>, Kind) {
    let mut parameters = Vec::new();
    let mut current = kind.clone();
    for _ in 0..arguments {
        match current {
            Kind::Function(parameter, result) => {
                parameters.push(*parameter);
                current = *result;
            }
            other => {
                parameters.push(other);
                current = Kind::Type;
            }
        }
    }
    (parameters, current)
}
