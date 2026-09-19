use super::*;
use crate::kind::{builtin_type_kind, flatten_spine, occurs, substitute};
use psrs_hir::BuiltinType;

impl Checker<'_> {
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
        for declaration in &self.module.types {
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
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| self.fresh()),
            TypeKind::Constructor(builtin) => match builtin {
                BuiltinType::Type => Kind::Type,
                BuiltinType::Constraint => Kind::Constraint,
                BuiltinType::Symbol => Kind::Symbol,
                BuiltinType::Row => Kind::Row,
                other => Kind::Builtin(*other),
            },
            TypeKind::Named(id) => Kind::Named(*id),
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

    // ----- Kind inference ----------------------------------------------

    fn kind_of_type(&mut self, ty: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        match &ty.kind {
            TypeKind::Application(..) => {
                let (head, arguments) = flatten_spine(ty);
                self.check_partial_synonym(head, arguments.len(), ty.span);
                let mut function = self.head_kind(head, scope);
                for argument in arguments {
                    let argument = self.kind_of_type(argument, scope);
                    let result = self.fresh();
                    self.unify(
                        function,
                        Kind::Function(Box::new(argument), Box::new(result.clone())),
                        ty.span,
                    );
                    function = result;
                }
                function
            }
            TypeKind::Named(id) => {
                self.check_partial_synonym(ty, 0, ty.span);
                self.instantiate_named(*id)
            }
            TypeKind::Constructor(BuiltinType::Function) => {
                self.check_partial_synonym(ty, 0, ty.span);
                builtin_type_kind(BuiltinType::Function)
            }
            _ => self.kind_of_atom(ty, scope),
        }
    }

    fn head_kind(&mut self, head: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        match &head.kind {
            TypeKind::Named(id) => self.instantiate_named(*id),
            TypeKind::Constructor(builtin) => builtin_type_kind(*builtin),
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| self.fresh()),
            _ => self.kind_of_atom(head, scope),
        }
    }

    fn kind_of_atom(&mut self, ty: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        match &ty.kind {
            TypeKind::Variable(name) => scope.get(name).cloned().unwrap_or_else(|| {
                let kind = self.fresh();
                scope.insert(name.clone(), kind.clone());
                kind
            }),
            TypeKind::Constructor(builtin) => builtin_type_kind(*builtin),
            TypeKind::Named(id) => self.instantiate_named(*id),
            TypeKind::Application(..) => self.kind_of_type(ty, scope),
            TypeKind::Function { parameter, result } => {
                let parameter_kind = self.kind_of_type(parameter, scope);
                self.unify(parameter_kind, Kind::Type, parameter.span);
                let result_kind = self.kind_of_type(result, scope);
                self.unify(result_kind, Kind::Type, result.span);
                Kind::Type
            }
            TypeKind::Forall { variables, body } => {
                let saved = scope.clone();
                for variable in variables {
                    let kind = match &variable.kind {
                        Some(annotation) => self.denote_kind(annotation, scope),
                        None => self.fresh(),
                    };
                    scope.insert(variable.name.clone(), kind);
                }
                let kind = self.kind_of_type(body, scope);
                *scope = saved;
                kind
            }
            TypeKind::Constrained { constraint, body } => {
                let constraint_kind = self.kind_of_type(constraint, scope);
                self.unify(constraint_kind, Kind::Constraint, constraint.span);
                self.kind_of_type(body, scope)
            }
            TypeKind::Row { fields, tail } => {
                let field_kind = self.fresh();
                for field in fields {
                    let kind = self.kind_of_type(&field.ty, scope);
                    self.unify(kind, field_kind.clone(), field.ty.span);
                }
                if let Some(tail) = tail {
                    let tail_kind = self.kind_of_type(tail, scope);
                    self.unify(
                        tail_kind,
                        Kind::App(Box::new(Kind::Row), Box::new(field_kind.clone())),
                        tail.span,
                    );
                }
                Kind::App(Box::new(Kind::Row), Box::new(field_kind))
            }
            TypeKind::Record { fields, tail } => {
                let field_kind = self.fresh();
                for field in fields {
                    let kind = self.kind_of_type(&field.ty, scope);
                    self.unify(kind, field_kind.clone(), field.ty.span);
                }
                if let Some(tail) = tail {
                    let tail_kind = self.kind_of_type(tail, scope);
                    self.unify(
                        tail_kind,
                        Kind::App(Box::new(Kind::Row), Box::new(field_kind)),
                        tail.span,
                    );
                }
                Kind::Type
            }
            TypeKind::Integer(_) => Kind::Builtin(BuiltinType::Int),
            TypeKind::String(_) => Kind::Symbol,
        }
    }

    fn check_partial_synonym(&mut self, head: &hir::Type, arguments: usize, span: TextRange) {
        match &head.kind {
            TypeKind::Named(id) => {
                if let Some(arity) = self.synonym_arity.get(id)
                    && arguments < *arity
                {
                    self.report(
                        PARTIALLY_APPLIED_SYNONYM,
                        span,
                        "a type synonym must be fully applied",
                    );
                }
            }
            TypeKind::Constructor(BuiltinType::Function) if arguments < 2 => {
                self.report(
                    PARTIALLY_APPLIED_SYNONYM,
                    span,
                    "a type synonym must be fully applied",
                );
            }
            _ => {}
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
            }
        }
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
