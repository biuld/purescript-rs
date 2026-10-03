use super::*;
use crate::kind::{flatten_spine, type_kind};
use crate::solve::substitute;

mod pattern_annotations;
mod type_kind;

impl Checker<'_> {
    /// The zonked schemes of every declaration this run checked.
    ///
    /// A kind variable the declaration's own definition left undetermined is
    /// quantified here: the scheme generalizes exactly the unknowns its
    /// definition does not determine, and keeps the kinds it does. A parameter
    /// its own fields determine — `f` in `data Apply f = Apply (f Int)` — carries
    /// no variable left, so it is published as the kind the definition inferred
    /// rather than as something to generalize. Generalizing it instead would lose
    /// the occurs check, which is what rejects `data Branch m = Branch (m Branch)`.
    pub(super) fn checked_schemes(&self) -> HashMap<TypeId, KindScheme> {
        self.schemes
            .iter()
            .map(|(id, scheme)| {
                let kind = self.state.resolve(scheme.kind.clone());
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

    // ----- Scheme construction ----------------------------------------

    /// The kind of a referenced declaration at this use.
    ///
    /// A declaration with a checked scheme is instantiated, so a use of a
    /// polymorphic declaration is independent of every other use. A declaration
    /// with *no* checked scheme is the program not being closed over its type
    /// declarations: that is reported, and official PureScript reports it the
    /// same way, because `elaborateKind` raises `UnknownName` for a constructor
    /// it cannot resolve instead of inventing a variable. The variable keeps
    /// repeated uses of the same declaration in agreement for the rest of this
    /// run so one missing interface produces one diagnostic.
    fn instantiate_named(&mut self, id: TypeId, span: TextRange) -> Kind {
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
                let name = self.names.get(&id).cloned().unwrap_or_default();
                self.report_about(
                    id,
                    MISSING_KIND_METADATA,
                    span,
                    format!(
                        "the kind of `{name}` is unknown: the program has no checked kind signature for it"
                    ),
                );
                let kind = self.fresh();
                self.schemes
                    .insert(id, KindScheme::monomorphic(kind.clone()));
                kind
            }
        }
    }

    /// Registers every declaration head and kind signature this run knows.
    ///
    /// A declared signature is read as a kind; an absent one is inferred from
    /// the declaration's parameters, with a kind variable for each parameter the
    /// definition leaves undetermined.
    pub(super) fn build_schemes(&mut self) {
        let primitives = psrs_hir::primitive_type_declarations();
        let declarations = self
            .modules
            .iter()
            .flat_map(|module| module.types.iter())
            .chain(primitives.iter().map(|(_, declaration)| declaration));
        for declaration in declarations {
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
                            Some(annotation) => self.denote_annotation(annotation, &mut scope),
                            None => self.fresh(),
                        };
                        scope.insert(parameter.name.clone(), kind.clone());
                        parameters.push(kind);
                    }
                    let result = match declaration.kind {
                        TypeDeclarationKind::Data | TypeDeclarationKind::Newtype => type_kind(),
                        TypeDeclarationKind::Class => constraint_kind(),
                        TypeDeclarationKind::TypeSynonym => self.fresh(),
                        // A foreign data declaration always carries its kind.
                        // This arm is only the fallback when that kind is absent.
                        TypeDeclarationKind::Foreign => type_kind(),
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

    /// Reads a standalone kind signature, quantifying its `forall` binders.
    ///
    /// Each binder becomes a rigid kind variable, so the annotation is checked
    /// against the polymorphic kind it declares rather than against a variable
    /// the annotation itself may bind.
    fn parse_declared_kind(&mut self, signature: &hir::Type) -> (Vec<u32>, Kind) {
        // The whole signature is walked for an unsaturated synonym once, so the
        // binders below read their annotations through `denote` alone.
        self.check_annotation_saturation(signature);
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
                    let kind = self.denote(annotation, &mut scope);
                    scope.insert(binder.name.clone(), kind);
                }
                let Kind::Variable(id) = self.fresh() else {
                    unreachable!("a fresh kind is a variable")
                };
                self.state.make_rigid(id);
                scope.insert(binder.name.clone(), Kind::Variable(id));
                variables.push(id);
            }
            body = inner;
        }
        let kind = self.denote(body, &mut scope);
        (variables, kind)
    }

    // ----- Definition checking -----------------------------------------

    pub(super) fn check_definitions(&mut self) {
        for module in self.modules {
            self.current = module.id;
            self.check_module_definitions(module);
        }
    }

    fn check_module_definitions(&mut self, module: &hir::Module) {
        for declaration in &module.declarations {
            if let Some(signature) = &declaration.signature {
                let mut scope = HashMap::new();
                let kind = self.kind_of_type(signature, &mut scope);
                self.unify(kind, type_kind(), signature.span);
            }
        }
        for declaration in &module.types {
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
                            self.unify(kind, type_kind(), field.span);
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
                        self.unify(kind, constraint_kind(), superclass.span);
                    }
                    for member in &declaration.members {
                        if let Some(signature) = &member.signature {
                            let kind = self.kind_of_type(signature, &mut scope);
                            self.unify(kind, type_kind(), signature.span);
                        }
                    }
                }
                TypeDeclarationKind::Foreign => {}
            }
        }
        self.check_instance_heads(module);
        self.check_local_type_annotations(module);
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
    fn check_instance_heads(&mut self, module: &hir::Module) {
        for instance in &module.instances {
            // A head that applies its class to the wrong number of arguments is
            // an arity error, not a kind error, and the class environment owns
            // that rule. Checking it here too would report `KindsDoNotUnify`
            // where `ClassInstanceArityMismatch` is the official code, and would
            // pre-empt the arity diagnostic entirely. A class this module does
            // not declare has no known arity here either, so it is left alone.
            let Some(declared) = module
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
            self.unify(kind, constraint_kind(), instance.head.span);
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
        Kind::Builtin(_) | Kind::Named(_) => {}
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
                current = type_kind();
            }
        }
    }
    (parameters, current)
}
