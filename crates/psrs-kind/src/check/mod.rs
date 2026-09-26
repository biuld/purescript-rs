use crate::kind::{Kind, KindDiagnostic, KindScheme, collect_type_ids};
use psrs_hir::{self as hir, TypeDeclarationKind, TypeId, TypeKind};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod infer;

/// Official `errorCode`s this pass reports.
pub const KINDS_DO_NOT_UNIFY: &str = "KindsDoNotUnify";
pub const INFINITE_KIND: &str = "InfiniteKind";
pub const PARTIALLY_APPLIED_SYNONYM: &str = "PartiallyAppliedSynonym";
pub const CYCLE_IN_TYPE_SYNONYM: &str = "CycleInTypeSynonym";
pub const CYCLE_IN_KIND_DECLARATION: &str = "CycleInKindDeclaration";
pub const UNDEFINED_TYPE_VARIABLE: &str = "UndefinedTypeVariable";

/// Checks the kinds of a resolved module's type declarations and signatures.
pub fn check_module(module: &hir::Module) -> Vec<KindDiagnostic> {
    let mut checker = Checker::new(module);
    checker.run();
    checker.errors
}

struct Checker<'a> {
    module: &'a hir::Module,
    schemes: HashMap<TypeId, KindScheme>,
    synonym_arity: HashMap<TypeId, usize>,
    substitutions: HashMap<u32, Kind>,
    rigid: HashSet<u32>,
    next_var: u32,
    errors: Vec<KindDiagnostic>,
}

impl<'a> Checker<'a> {
    fn new(module: &'a hir::Module) -> Self {
        let mut synonym_arity = HashMap::new();
        for declaration in &module.types {
            if declaration.kind == TypeDeclarationKind::TypeSynonym {
                synonym_arity.insert(declaration.id, declaration.parameters.len());
            }
        }
        Self {
            module,
            schemes: HashMap::new(),
            synonym_arity,
            substitutions: HashMap::new(),
            rigid: HashSet::new(),
            next_var: 0,
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
        let variable = self.next_var;
        self.next_var += 1;
        Kind::Variable(variable)
    }

    fn report(&mut self, code: &'static str, span: TextRange, message: impl Into<String>) {
        self.errors.push(KindDiagnostic::new(code, span, message));
    }

    // ----- Undefined type variables ------------------------------------

    fn check_defined_variables(&mut self) {
        for declaration in &self.module.declarations {
            if let Some(signature) = &declaration.signature {
                let mut bound = Vec::new();
                self.check_references(signature, &mut bound);
            }
        }
        for declaration in &self.module.types {
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
            TypeKind::Constructor(_)
            | TypeKind::Named(_)
            | TypeKind::Opaque(_)
            | TypeKind::Integer(_)
            | TypeKind::String(_) => {}
        }
    }

    // ----- Cycle detection ---------------------------------------------

    fn check_cycles(&mut self) {
        let mut synonym_edges: HashMap<TypeId, Vec<(TypeId, TextRange)>> = HashMap::new();
        let mut kind_edges: HashMap<TypeId, Vec<(TypeId, TextRange)>> = HashMap::new();
        let mut kind_nodes = Vec::new();
        for declaration in &self.module.types {
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
        for (id, span) in crate::kind::detect_cycle(synonym_nodes, &synonym_edges) {
            let name = self
                .module
                .types
                .iter()
                .find(|declaration| declaration.id == id)
                .map(|declaration| declaration.name.clone())
                .unwrap_or_default();
            self.report(
                CYCLE_IN_TYPE_SYNONYM,
                span,
                format!("the type synonym `{name}` is defined in terms of itself"),
            );
        }
        for (id, span) in crate::kind::detect_cycle(kind_nodes, &kind_edges) {
            let name = self
                .module
                .types
                .iter()
                .find(|declaration| declaration.id == id)
                .map(|declaration| declaration.name.clone())
                .unwrap_or_default();
            self.report(
                CYCLE_IN_KIND_DECLARATION,
                span,
                format!("the kind declaration for `{name}` is circular"),
            );
        }
    }
}
