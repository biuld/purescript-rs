use super::*;
use crate::kind::{flatten_spine, type_kind};
use crate::solve::substitute;

mod binding_group;
mod closed_type;
mod pattern_annotations;
mod quantify;
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
                let binders = scheme
                    .binders
                    .iter()
                    .map(|binder| KindBinder {
                        variable: binder.variable,
                        kind: self.state.resolve(binder.kind.clone()),
                    })
                    .collect();
                (
                    *id,
                    KindScheme {
                        variables,
                        kind,
                        binders,
                    },
                )
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
                for variable in &scheme.variables {
                    // A binder that does not occur in the kind is the sort of
                    // another binder. Allocating it must not look like an
                    // unknown left in the type: official quantification only
                    // sees unknowns that the elaborated type still contains.
                    let fresh = if crate::solve::occurs(*variable, &scheme.kind) {
                        self.fresh_closed_unknown()
                    } else {
                        self.fresh()
                    };
                    mapping.insert(*variable, fresh);
                }
                substitute(&scheme.kind, &mapping)
            }
            // An inferred scheme's parameter kinds are still flexible. A use
            // outside this binding group instantiates them, so one field cannot
            // solve another declaration's parameter. Uses inside the group keep
            // the shared kind: that is what solves `A` from `B`, and what the
            // occurs check reads.
            Some(scheme) if self.in_closed_type && !self.same_kind_group(id) => {
                self.instantiate_inferred(&scheme)
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
                    let (variables, kind, binders) =
                        self.parse_declared_kind(declaration.id, signature);
                    KindScheme {
                        variables,
                        kind,
                        binders,
                    }
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
    fn parse_declared_kind(
        &mut self,
        id: hir::TypeId,
        signature: &hir::Type,
    ) -> (Vec<u32>, Kind, Vec<KindBinder>) {
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
        // The denoted spine is what uses instantiate. The binder list is the
        // separate implicit-before-explicit order, with each binder's sort,
        // and it is what rejects an ill-scoped generalization.
        let binders = self.signature_binders(id, signature, &variables);
        (variables, kind, binders)
    }

    // ----- Definition checking -----------------------------------------

    pub(super) fn check_definitions(&mut self) {
        for module in self.modules {
            self.current = module.id;
            self.check_declared_types(module);
        }
        // A declaration's scheme quantifies the unknowns its own definition
        // left free. Value signatures, instance heads, and expression
        // annotations instantiate that scheme, so one use cannot solve
        // another use's copy of the same parameter.
        let generalized = self.checked_schemes();
        self.schemes = generalized;
        for module in self.modules {
            self.current = module.id;
            self.check_uses(module);
        }
    }

    /// Solves a type-annotation kind equation, reporting `ExpectedType` when
    /// it fails.
    ///
    /// A value's written type must have kind `Type`, and at an annotation
    /// (`x :: t`, a value signature, a binder annotation) official PureScript
    /// solves exactly this equation through `checkTypeKind`
    /// (`TypeChecker/Kinds.hs`), whose failure handler reports `ExpectedType`
    /// with the written type and its kind. The kind unifier's own mismatch is
    /// reserved for kinds two types impose on each other, so an annotation
    /// that is not a type at all is not reported as a conflict between kinds.
    ///
    /// A failure may leave partial bindings behind; like the official
    /// `unifyKindsWithFailure` failure handler, they stay and the report names
    /// the kind the annotation carried when its equation failed.
    pub(super) fn check_type_annotation_kind(
        &mut self,
        kind: Kind,
        against: Kind,
        span: TextRange,
    ) {
        if crate::solve::unify_kind(&mut self.state, kind.clone(), against, span).is_err() {
            let kind = self.state.resolve(kind);
            self.report(
                EXPECTED_TYPE,
                span,
                format!(
                    "in a type-annotated expression `x :: t`, the type `t` must have kind `Type`; it has kind `{kind:?}`"
                ),
            );
        }
    }

    fn check_uses(&mut self, module: &hir::Module) {
        for signature in module.declared_value_signatures() {
            let mut scope = HashMap::new();
            let kind = self.kind_of_type(signature, &mut scope);
            self.check_type_annotation_kind(kind, type_kind(), signature.span);
        }
        self.check_instance_heads(module);
        self.check_local_type_annotations(module);
    }

    fn check_declared_types(&mut self, module: &hir::Module) {
        let (groups, order) = binding_group::order(module);
        self.kind_group = groups;
        let position: HashMap<TypeId, usize> = module
            .types
            .iter()
            .enumerate()
            .map(|(index, declaration)| (declaration.id, index))
            .collect();
        for id in order {
            let Some(&index) = position.get(&id) else {
                continue;
            };
            self.current_type = Some(id);
            let declaration = &module.types[index];
            let Some(scheme) = self.schemes.get(&declaration.id).cloned() else {
                continue;
            };
            let (parameters, result) = strip_function(&scheme.kind, declaration.parameters.len());
            self.guarded_kinds.clear();
            for kind in &parameters {
                collect_kind_variables(kind, &mut self.guarded_kinds);
            }
            let mut scope = HashMap::new();
            for (parameter, kind) in declaration.parameters.iter().zip(parameters) {
                scope.insert(parameter.name.clone(), kind);
            }
            match declaration.kind {
                TypeDeclarationKind::Data | TypeDeclarationKind::Newtype => {
                    for constructor in &declaration.constructors {
                        for field in &constructor.fields {
                            let kind = self.kind_of_closed_type(field, &mut scope);
                            self.unify(kind, type_kind(), field.span);
                            self.report_closed_unknowns(field.span);
                        }
                    }
                }
                TypeDeclarationKind::TypeSynonym => {
                    if let Some(body) = &declaration.body {
                        self.in_synonym = true;
                        let kind = self.kind_of_closed_type(body, &mut scope);
                        self.in_synonym = false;
                        self.unify(kind, result, body.span);
                        self.report_closed_unknowns(body.span);
                        self.report_visible_type_quantification(body.span);
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
        self.current_type = None;
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
    ///
    /// The class does not have to be declared in the module that declares the
    /// instance: a head whose class is foreign is still checked here against the
    /// scheme its declaring module's run produced, because a program-level run
    /// sees every module's scheme and a module-level run receives the imported
    /// ones. What does not need to be the same is the arity source. A class this
    /// module declares has its parameter list at hand; a foreign class is read
    /// through its scheme (its kind is one arrow per parameter ending in
    /// `Constraint`), and a scheme this run does not know leaves the head
    /// unguarded, because a signature the program never checked must not be
    /// replaced by an invention.
    fn check_instance_heads(&mut self, module: &hir::Module) {
        for instance in &module.instances {
            // A head that applies its class to the wrong number of arguments is
            // an arity error, not a kind error, and the class environment owns
            // that rule. Checking it here too would report `KindsDoNotUnify`
            // where `ClassInstanceArityMismatch` is the official code, and would
            // pre-empt the arity diagnostic entirely.
            let declared = module
                .types
                .iter()
                .find(|declaration| declaration.id == instance.class_id);
            let parameters = match declared {
                Some(declaration) => Some(declaration.parameters.len()),
                None => self.scheme_parameter_count(instance.class_id),
            };
            let (_, arguments) = flatten_spine(&instance.head);
            let Some(parameters) = parameters else {
                continue;
            };
            if arguments.len() != parameters {
                continue;
            }
            let mut scope = HashMap::new();
            let kind = self.kind_of_type_against(&instance.head, &mut scope, false);
            self.unify(kind, constraint_kind(), instance.head.span);
        }
    }

    /// How many parameters a class's checked kind scheme takes.
    ///
    /// The scheme's kind is one function per parameter ending in `Constraint`,
    /// so the count is the chain of function steps. `None` says this run has no
    /// scheme for the class — a program this run is not closed over — and the
    /// head is then left to whatever the scheme's absence reports elsewhere.
    fn scheme_parameter_count(&self, id: TypeId) -> Option<usize> {
        let scheme = self.schemes.get(&id)?;
        let mut count = 0;
        let mut kind = &scheme.kind;
        while let Kind::Function(_, result) = kind {
            count += 1;
            kind = result;
        }
        Some(count)
    }
}

pub(super) fn collect_kind_variables(kind: &Kind, out: &mut Vec<u32>) {
    fn walk(kind: &Kind, bound: &mut Vec<u32>, out: &mut Vec<u32>) {
        match kind {
            Kind::Variable(variable) if !bound.contains(variable) => out.push(*variable),
            Kind::Variable(_) => {}
            Kind::App(function, argument) | Kind::Function(function, argument) => {
                walk(function, bound, out);
                walk(argument, bound, out);
            }
            Kind::RowExtend { field, tail, .. } => {
                walk(field, bound, out);
                walk(tail, bound, out);
            }
            Kind::ForAll { variable, body } => {
                bound.push(*variable);
                walk(body, bound, out);
                bound.pop();
            }
            Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty | Kind::TypeVariable(_) => {}
        }
    }
    walk(kind, &mut Vec::new(), out);
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
