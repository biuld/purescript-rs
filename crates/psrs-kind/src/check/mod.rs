use crate::denote::{KindScope, denote_kind};
use crate::kind::{
    CheckedKindEnv, Kind, KindDiagnostic, KindScheme, collect_type_ids, constraint_kind,
    detect_cycle,
};
use crate::solve::KindState;
use psrs_hir::{self as hir, ModuleId, Role, TypeDeclarationKind, TypeId, TypeKind};
use psrs_span::TextRange;
use std::collections::HashMap;

mod infer;
mod roles;

/// Official `errorCode`s this pass reports.
pub const KINDS_DO_NOT_UNIFY: &str = "KindsDoNotUnify";
pub const INFINITE_KIND: &str = "InfiniteKind";
pub const PARTIALLY_APPLIED_SYNONYM: &str = "PartiallyAppliedSynonym";
pub const CYCLE_IN_TYPE_SYNONYM: &str = "CycleInTypeSynonym";
pub const CYCLE_IN_KIND_DECLARATION: &str = "CycleInKindDeclaration";
pub const UNDEFINED_TYPE_VARIABLE: &str = "UndefinedTypeVariable";
/// A referenced type that the program never checked, so no kind is known for
/// it. This is the code official PureScript reports when `elaborateKind` cannot
/// resolve a constructor: a missing kind is not an inferable parameter.
pub const MISSING_KIND_METADATA: &str = "UnknownName";
/// A type expression that reached kind checking before P4 lowered it.
pub const UNLOWERED_TYPE_OPERATOR: &str = "UnloweredTypeOperator";

/// Checks the kinds of a whole program once, and returns the environment every
/// module's type check consumes.
///
/// This is the program-level entry point. It registers the primitive kind table
/// and every declaration head, rejects synonym and kind-declaration cycles,
/// infers declaration kinds, infers roles to a fixed point, and returns one
/// zonked environment together with every diagnostic, each attributed to the
/// module that declares the offending type.
pub fn check_program(modules: &[hir::Module]) -> (CheckedKindEnv, Vec<KindDiagnostic>) {
    let mut checker = Checker::for_program(modules);
    checker.run();
    let mut diagnostics = std::mem::take(&mut checker.errors);
    let (roles, role_diagnostics) = roles::infer_roles_fixed_point(modules);
    diagnostics.extend(role_diagnostics);
    let environment = checker.checked_environment(roles);
    (environment, diagnostics)
}

/// Re-checks one module's own declarations, annotations, and instance heads
/// against the schemes [`check_program`] already produced.
///
/// An imported declaration keeps the kind its declaring module checked: this
/// never re-derives it. A referenced declaration with no checked scheme is a
/// diagnostic rather than a fabricated kind variable, because absent interface
/// metadata must not read as an inferable parameter.
pub fn check_module(module: &hir::Module, imported: &CheckedKindEnv) -> Vec<KindDiagnostic> {
    let mut checker = Checker::for_module(module, imported);
    checker.run();
    checker.errors
}

struct Checker<'a> {
    /// Every module this run knows, in source order. A program-level run sees
    /// them all, so one equation can constrain a declaration in another module.
    modules: &'a [hir::Module],
    /// The module whose own declarations are being checked right now. A
    /// diagnostic about this module's syntax names it.
    current: ModuleId,
    /// The module that declares each type, so a use in one module is checked
    /// against — and reported against — the module that declared the type.
    declaring: HashMap<TypeId, ModuleId>,
    /// The name each declared type has, for a diagnostic that names it.
    names: HashMap<TypeId, String>,
    /// The schemes this run knows: the program's checked schemes, overwritten by
    /// the declarations this run checks.
    schemes: HashMap<TypeId, KindScheme>,
    synonym_arity: HashMap<TypeId, usize>,
    state: KindState,
    errors: Vec<KindDiagnostic>,
}

impl<'a> Checker<'a> {
    fn for_program(modules: &'a [hir::Module]) -> Self {
        let (declaring, names) = declaration_index(modules);
        Self {
            current: modules
                .first()
                .map(|module| module.id)
                .unwrap_or(ModuleId(0)),
            modules,
            declaring,
            names,
            schemes: HashMap::new(),
            synonym_arity: synonym_arities(modules),
            state: coercible_state(),
            errors: Vec::new(),
        }
    }

    fn for_module(module: &'a hir::Module, imported: &CheckedKindEnv) -> Self {
        let modules = std::slice::from_ref(module);
        let (declaring, names) = declaration_index(modules);
        let mut schemes = imported.kinds.clone();
        schemes.extend(coercible_scheme());
        Self {
            current: module.id,
            modules,
            declaring,
            names,
            schemes,
            synonym_arity: synonym_arities(modules),
            state: coercible_state(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        self.check_defined_variables();
        self.check_cycles();
        self.build_schemes();
        self.check_definitions();
    }

    fn fresh(&mut self) -> Kind {
        self.state.fresh()
    }

    /// Solves a kind equation through the one solver and reports its failure.
    fn unify(&mut self, left: Kind, right: Kind, span: TextRange) {
        if let Err(error) = crate::solve::unify_kind(&mut self.state, left, right, span) {
            self.report(error.code, span, error.message);
        }
    }

    fn report(&mut self, code: &'static str, span: TextRange, message: impl Into<String>) {
        self.errors
            .push(KindDiagnostic::new(self.current, code, span, message));
    }

    /// Reports a problem with a type the module `id` declares, so the
    /// diagnostic lands on the declaration's own source even when another
    /// module's use exposed it.
    fn report_about(
        &mut self,
        id: TypeId,
        code: &'static str,
        span: TextRange,
        message: impl Into<String>,
    ) {
        let origin = self.declaring.get(&id).copied().unwrap_or(self.current);
        self.errors
            .push(KindDiagnostic::new(origin, code, span, message));
    }

    fn checked_environment(&self, roles: HashMap<TypeId, Vec<Role>>) -> CheckedKindEnv {
        let mut declaring_module = self.declaring.clone();
        for (_, declaration) in psrs_hir::primitive_type_declarations() {
            declaring_module.insert(declaration.id, ModuleId::INTRINSICS);
        }
        CheckedKindEnv {
            roles,
            kinds: self.checked_schemes(),
            declaring_module,
        }
    }

    // ----- Kind denotation ---------------------------------------------

    /// Reads a kind annotation, rejecting an unsaturated synonym in it first.
    fn denote_annotation(&mut self, ty: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        self.check_annotation_saturation(ty);
        self.denote(ty, scope)
    }

    /// Reads a type expression as a kind, allocating through the one solver.
    ///
    /// This is [`denote_kind`] with the bindings a caller has accumulated as a
    /// plain map, so the surrounding checker does not have to lend out its
    /// solver while it is already borrowed.
    fn denote(&mut self, ty: &hir::Type, scope: &mut HashMap<String, Kind>) -> Kind {
        let mut kind_scope = KindScope::with_bindings(&mut self.state, std::mem::take(scope));
        let kind = denote_kind(ty, &mut kind_scope);
        *scope = kind_scope.into_bindings();
        kind.unwrap_or_else(|| {
            self.report(
                UNLOWERED_TYPE_OPERATOR,
                ty.span,
                "type operator chain reached kind denotation before P4",
            );
            self.fresh()
        })
    }

    // ----- Undefined type variables ------------------------------------

    fn check_defined_variables(&mut self) {
        for module in self.modules {
            self.current = module.id;
            for declaration in &module.declarations {
                if let Some(signature) = &declaration.signature {
                    let mut bound = Vec::new();
                    self.check_references(signature, &mut bound);
                }
            }
            for declaration in &module.types {
                let mut bound: Vec<String> = Vec::new();
                if let Some(signature) = &declaration.declared_kind {
                    self.check_references(signature, &mut bound);
                    bound.clear();
                }
                for parameter in &declaration.parameters {
                    if let Some(annotation) = &parameter.kind {
                        self.check_references(annotation, &mut bound);
                    }
                    bound.push(parameter.name.clone());
                }
                match declaration.kind {
                    TypeDeclarationKind::Data | TypeDeclarationKind::Newtype => {
                        for constructor in &declaration.constructors {
                            for field in &constructor.fields {
                                self.check_references(field, &mut bound);
                            }
                        }
                    }
                    TypeDeclarationKind::TypeSynonym => {
                        if let Some(body) = &declaration.body {
                            self.check_references(body, &mut bound);
                        }
                    }
                    TypeDeclarationKind::Class => {
                        for superclass in &declaration.superclasses {
                            self.check_references(superclass, &mut bound);
                        }
                        for member in &declaration.members {
                            if let Some(signature) = &member.signature {
                                self.check_references(signature, &mut bound);
                            }
                        }
                    }
                    TypeDeclarationKind::Foreign => {}
                }
            }
        }
    }

    fn check_references(&mut self, ty: &hir::Type, bound: &mut Vec<String>) {
        match &ty.kind {
            TypeKind::Variable(name) => {
                if !bound.iter().any(|bound| bound == name) {
                    self.report(
                        UNDEFINED_TYPE_VARIABLE,
                        ty.span,
                        format!("the type variable `{name}` is undefined"),
                    );
                }
            }
            TypeKind::Application(function, argument) => {
                self.check_references(function, bound);
                self.check_references(argument, bound);
            }
            TypeKind::OperatorChain { operands, .. } => {
                for operand in operands {
                    self.check_references(operand, bound);
                }
            }
            TypeKind::Function { parameter, result } => {
                self.check_references(parameter, bound);
                self.check_references(result, bound);
            }
            TypeKind::Forall { variables, body } => {
                let saved = bound.len();
                for variable in variables {
                    if let Some(annotation) = &variable.kind {
                        self.check_references(annotation, bound);
                    }
                    bound.push(variable.name.clone());
                }
                self.check_references(body, bound);
                bound.truncate(saved);
            }
            TypeKind::Constrained { constraint, body } => {
                self.check_references(constraint, bound);
                self.check_references(body, bound);
            }
            TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
                for field in fields {
                    self.check_references(&field.ty, bound);
                }
                if let Some(tail) = tail {
                    self.check_references(tail, bound);
                }
            }
            TypeKind::Wildcard
            | TypeKind::Constructor(_)
            | TypeKind::Named(_)
            | TypeKind::Opaque(_)
            | TypeKind::Integer(_)
            | TypeKind::String(_) => {}
        }
    }

    // ----- Cycle detection ---------------------------------------------

    fn check_cycles(&mut self) {
        for module in self.modules {
            self.current = module.id;
            let mut synonym_edges: HashMap<TypeId, Vec<(TypeId, TextRange)>> = HashMap::new();
            let mut kind_edges: HashMap<TypeId, Vec<(TypeId, TextRange)>> = HashMap::new();
            let mut kind_nodes = Vec::new();
            for declaration in &module.types {
                if declaration.kind == TypeDeclarationKind::TypeSynonym
                    && let Some(body) = &declaration.body
                {
                    let mut referenced = Vec::new();
                    collect_type_ids(body, &mut referenced);
                    synonym_edges.insert(
                        declaration.id,
                        referenced
                            .into_iter()
                            .map(|id| (id, declaration.name_span))
                            .collect(),
                    );
                }
                if let Some(signature) = &declaration.declared_kind {
                    kind_nodes.push(declaration.id);
                    let mut referenced = Vec::new();
                    collect_type_ids(signature, &mut referenced);
                    kind_edges.insert(
                        declaration.id,
                        referenced
                            .into_iter()
                            .map(|id| (id, declaration.name_span))
                            .collect(),
                    );
                }
            }
            let synonym_nodes: Vec<TypeId> = synonym_edges.keys().copied().collect();
            for (id, span) in detect_cycle(synonym_nodes, &synonym_edges) {
                let name = self.names.get(&id).cloned().unwrap_or_default();
                self.report(
                    CYCLE_IN_TYPE_SYNONYM,
                    span,
                    format!("the type synonym `{name}` is defined in terms of itself"),
                );
            }
            for (id, span) in detect_cycle(kind_nodes, &kind_edges) {
                let name = self.names.get(&id).cloned().unwrap_or_default();
                self.report(
                    CYCLE_IN_KIND_DECLARATION,
                    span,
                    format!("the kind declaration for `{name}` is circular"),
                );
            }
        }
    }
}

/// The compiler-owned scheme for the `Coercible` class, which is referenced
/// without being declared in any source module. Variable 0 is reserved for its
/// polymorphic kind parameter so no allocation reuses it.
fn coercible_scheme() -> [(TypeId, KindScheme); 1] {
    let variable = 0;
    [(
        TypeId::COERCIBLE,
        KindScheme {
            variables: vec![variable],
            kind: Kind::Function(
                Box::new(Kind::Variable(variable)),
                Box::new(Kind::Function(
                    Box::new(Kind::Variable(variable)),
                    Box::new(constraint_kind()),
                )),
            ),
        },
    )]
}

fn coercible_state() -> KindState {
    KindState::starting_at(1)
}

fn declaration_index(
    modules: &[hir::Module],
) -> (HashMap<TypeId, ModuleId>, HashMap<TypeId, String>) {
    let mut declaring = HashMap::new();
    let mut names = HashMap::new();
    for module in modules {
        for declaration in &module.types {
            declaring.insert(declaration.id, module.id);
            names.insert(declaration.id, declaration.name.clone());
        }
    }
    (declaring, names)
}

fn synonym_arities(modules: &[hir::Module]) -> HashMap<TypeId, usize> {
    let mut arities = HashMap::new();
    for module in modules {
        for declaration in &module.types {
            if declaration.kind == TypeDeclarationKind::TypeSynonym {
                arities.insert(declaration.id, declaration.parameters.len());
            }
        }
    }
    arities
}
