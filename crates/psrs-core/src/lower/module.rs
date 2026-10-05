use crate::{Declaration, LowerError, Module, Type, TypeId};
use std::collections::HashMap;

pub(super) struct LowerContext {
    next_local_id: u64,
}

impl LowerContext {
    fn new(module: &psrs_thir::Module) -> Self {
        let mut max_local_id = None;
        for declaration in &module.declarations {
            scan_expr_locals(&declaration.value, &mut max_local_id);
        }
        Self {
            next_local_id: max_local_id.map_or(0, |id| u64::from(id) + 1),
        }
    }

    pub(super) fn fresh_local(&mut self) -> Option<psrs_hir::LocalId> {
        let id = u32::try_from(self.next_local_id).ok()?;
        self.next_local_id += 1;
        Some(psrs_hir::LocalId(id))
    }
}

fn scan_expr_locals(expression: &psrs_thir::Expr, max: &mut Option<u32>) {
    let note = |id: psrs_hir::LocalId, max: &mut Option<u32>| {
        *max = Some(max.map_or(id.0, |current| current.max(id.0)));
    };
    match &expression.kind {
        psrs_thir::ExprKind::Local(id) => note(*id, max),
        psrs_thir::ExprKind::Global(_)
        | psrs_thir::ExprKind::Integer(_)
        | psrs_thir::ExprKind::Number(_)
        | psrs_thir::ExprKind::Boolean(_)
        | psrs_thir::ExprKind::String(_)
        | psrs_thir::ExprKind::Char(_) => {}
        psrs_thir::ExprKind::Array(elements) => {
            for element in elements {
                scan_expr_locals(element, max);
            }
        }
        psrs_thir::ExprKind::Record(fields) => {
            for (_, value) in fields {
                scan_expr_locals(value, max);
            }
        }
        psrs_thir::ExprKind::RecordUpdate { expression, fields } => {
            scan_expr_locals(expression, max);
            for (_, value) in fields {
                scan_expr_locals(value, max);
            }
        }
        psrs_thir::ExprKind::FieldAccess { expression, .. } => scan_expr_locals(expression, max),
        psrs_thir::ExprKind::Evidence(evidence) => scan_evidence_locals(evidence, max),
        psrs_thir::ExprKind::Coerce {
            value, evidence, ..
        } => {
            scan_expr_locals(value, max);
            scan_evidence_locals(evidence, max);
        }
        psrs_thir::ExprKind::UnsafeCoerce { value, .. } => scan_expr_locals(value, max),
        psrs_thir::ExprKind::Application(function, argument) => {
            scan_expr_locals(function, max);
            scan_expr_locals(argument, max);
        }
        psrs_thir::ExprKind::Lambda { binder, body } => {
            note(binder.id, max);
            scan_expr_locals(body, max);
        }
        psrs_thir::ExprKind::Let { bindings, body } => {
            for binding in bindings {
                note(binding.binder.id, max);
                scan_expr_locals(&binding.value, max);
            }
            scan_expr_locals(body, max);
        }
        psrs_thir::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            scan_expr_locals(condition, max);
            scan_expr_locals(then_branch, max);
            scan_expr_locals(else_branch, max);
        }
        psrs_thir::ExprKind::Case {
            scrutinee,
            branches,
        } => {
            scan_expr_locals(scrutinee, max);
            for branch in branches {
                scan_pattern_locals(&branch.pattern, max);
                scan_expr_locals(&branch.value, max);
            }
        }
    }
}

fn scan_pattern_locals(pattern: &psrs_thir::Pattern, max: &mut Option<u32>) {
    match &pattern.kind {
        psrs_thir::PatternKind::Wildcard | psrs_thir::PatternKind::Literal { .. } => {}
        psrs_thir::PatternKind::Array { elements } => {
            for element in elements {
                scan_pattern_locals(element, max);
            }
        }
        psrs_thir::PatternKind::Named { id, pattern } => {
            *max = Some(max.map_or(id.0, |current| current.max(id.0)));
            scan_pattern_locals(pattern, max);
        }
        psrs_thir::PatternKind::Var { id, .. } => {
            *max = Some(max.map_or(id.0, |current| current.max(id.0)));
        }
        psrs_thir::PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                scan_pattern_locals(argument, max);
            }
        }
        psrs_thir::PatternKind::Record { fields } => {
            for (_, pattern) in fields {
                scan_pattern_locals(pattern, max);
            }
        }
    }
}

fn scan_evidence_locals(evidence: &psrs_thir::Evidence, max: &mut Option<u32>) {
    match &evidence.kind {
        psrs_thir::EvidenceKind::Given(id) => {
            *max = Some(max.map_or(id.0, |current| current.max(id.0)));
        }
        psrs_thir::EvidenceKind::Superclass { parent, .. } => scan_evidence_locals(parent, max),
        psrs_thir::EvidenceKind::Instance { context, .. } => {
            for evidence in context {
                scan_evidence_locals(evidence, max);
            }
        }
        psrs_thir::EvidenceKind::Global(_)
        | psrs_thir::EvidenceKind::Coercible { .. }
        | psrs_thir::EvidenceKind::Primitive { .. } => {}
    }
}

fn lower_type_constructor(constructor: psrs_thir::TypeConstructor) -> crate::TypeConstructor {
    match constructor {
        psrs_thir::TypeConstructor::Function => crate::TypeConstructor::Function,
        psrs_thir::TypeConstructor::Record => crate::TypeConstructor::Record,
        psrs_thir::TypeConstructor::Row => crate::TypeConstructor::Row,
        psrs_thir::TypeConstructor::Array => crate::TypeConstructor::Array,
        psrs_thir::TypeConstructor::Int => crate::TypeConstructor::Int,
        psrs_thir::TypeConstructor::Number => crate::TypeConstructor::Number,
        psrs_thir::TypeConstructor::Boolean => crate::TypeConstructor::Boolean,
        psrs_thir::TypeConstructor::String => crate::TypeConstructor::String,
        psrs_thir::TypeConstructor::Char => crate::TypeConstructor::Char,
        psrs_thir::TypeConstructor::Unit => crate::TypeConstructor::Unit,
        psrs_thir::TypeConstructor::Type => crate::TypeConstructor::Type,
        psrs_thir::TypeConstructor::Constraint => crate::TypeConstructor::Constraint,
        psrs_thir::TypeConstructor::Symbol => crate::TypeConstructor::Symbol,
        psrs_thir::TypeConstructor::User(id) => crate::TypeConstructor::User(id),
    }
}

pub(super) fn lower_module_inner(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    if let Err(errors) = module.verify() {
        return Err(errors
            .into_iter()
            .map(|error| LowerError {
                span: error.span,
                message: "invalid THIR input",
            })
            .collect());
    }
    let externals = module
        .externals
        .iter()
        .map(|external| (external.symbol, external.kind.clone()))
        .collect::<HashMap<_, _>>();
    let constructors = module
        .constructors
        .iter()
        .map(|constructor| (constructor.symbol, constructor.clone()))
        .collect::<HashMap<_, _>>();
    let mut context = LowerContext::new(&module);
    let source_types = module.types;
    let types = source_types
        .iter()
        .cloned()
        .map(|ty| match ty {
            psrs_thir::Type::Variable(variable) => Type::Variable(variable),
            psrs_thir::Type::Constructor(constructor) => {
                Type::Constructor(lower_type_constructor(constructor))
            }
            psrs_thir::Type::Application(function, argument) => {
                Type::Application(TypeId(function.0), TypeId(argument.0))
            }
            psrs_thir::Type::ForAll { variables, body } => Type::ForAll {
                variables,
                body: TypeId(body.0),
            },
            psrs_thir::Type::RowEmpty => Type::RowEmpty,
            psrs_thir::Type::RowExtend { label, ty, tail } => Type::RowExtend {
                label,
                ty: TypeId(ty.0),
                tail: TypeId(tail.0),
            },
            psrs_thir::Type::TypeLevelString(value) => Type::TypeLevelString(value),
            psrs_thir::Type::TypeLevelInt(value) => Type::TypeLevelInt(value),
        })
        .collect();
    let mut declarations = Vec::with_capacity(module.declarations.len());
    for declaration in module.declarations {
        let value = super::lower_expr(
            declaration.value,
            &externals,
            &constructors,
            &source_types,
            &mut context,
        )
        .map_err(|error| vec![error])?;
        declarations.push(Declaration {
            symbol: declaration.symbol,
            name: declaration.name,
            name_span: declaration.name_span,
            quantified: declaration.quantified,
            ty: TypeId(declaration.ty.0),
            value,
            span: declaration.span,
        });
    }
    let lowered = Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        external_types: module
            .external_types
            .into_iter()
            .map(|external| crate::ExternalType {
                symbol: external.symbol,
                source_module: external.source_module,
                ty: TypeId(external.ty.0),
            })
            .collect(),
        types,
        newtype_ids: module.newtype_ids,
        opaque_ids: module.opaque_ids,
        callable_types: module.callable_types,
        constructors: module
            .constructors
            .iter()
            .map(|constructor| crate::ConstructorInfo {
                symbol: constructor.symbol,
                name: constructor.name.clone(),
                type_id: constructor.type_id,
                tag: constructor.tag,
                field_count: constructor.field_count,
                field_types: constructor
                    .field_types
                    .iter()
                    .map(|field| TypeId(field.0))
                    .collect(),
                parameters: constructor.parameters.clone(),
            })
            .collect(),
        declarations,
        type_names: module.type_names,
        entry: None,
        span: module.span,
    };
    Ok(lowered)
}
