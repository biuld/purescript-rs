use super::{ResolveError, ResolveErrorKind};
use psrs_ast::{self as ast, ExprKind as AstExprKind};
use psrs_hir::{
    self as hir, CaseBranchCoverage, Expr, ExprKind, ExternalSymbol, LocalBinder, LocalBinding,
    LocalId, ModuleId, SymbolId, TypeId, TypeReference,
};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod patterns;

struct QualifiedImport {
    module: ModuleId,
    values: HashMap<String, SymbolId>,
}

pub(super) struct QualifiedTypeImport {
    pub(super) module: ModuleId,
    pub(super) types: HashMap<String, TypeReference>,
}

pub(super) struct Resolver {
    pub(super) globals: HashMap<String, SymbolId>,
    pub(super) external_globals: HashMap<String, SymbolId>,
    pub(super) type_names: HashMap<String, TypeId>,
    /// Type IDs introduced by `foreign import data`, including imports. A
    /// reference to one of these is nominal and opaque.
    pub(super) opaque_types: HashSet<TypeId>,
    pub(super) imported_types: HashMap<String, Vec<TypeReference>>,
    pub(super) qualified_types: HashMap<String, Vec<QualifiedTypeImport>>,
    pub(super) externals: Vec<ExternalSymbol>,
    pub(super) imports: Vec<hir::Import>,
    pub(super) fixities: HashMap<String, hir::Fixity>,
    pub(super) type_fixities: HashMap<String, hir::Fixity>,
    pub(super) unqualified: HashMap<String, Vec<SymbolId>>,
    qualified: HashMap<String, Vec<QualifiedImport>>,
    pub(super) export_items: Option<ast::ExportList>,
    scopes: Vec<HashMap<String, LocalBinder>>,
    next_local: u32,
    pub(super) errors: Vec<ResolveError>,
    reported_conflicts: HashSet<String>,
}

impl Resolver {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        globals: HashMap<String, SymbolId>,
        external_globals: HashMap<String, SymbolId>,
        type_names: HashMap<String, TypeId>,
        externals: Vec<ExternalSymbol>,
        imports: Vec<hir::Import>,
        export_items: Option<ast::ExportList>,
        fixities: Vec<hir::Fixity>,
        errors: Vec<ResolveError>,
    ) -> Self {
        let mut unqualified: HashMap<String, Vec<SymbolId>> = HashMap::new();
        let mut qualified: HashMap<String, Vec<QualifiedImport>> = HashMap::new();
        let mut imported_types: HashMap<String, Vec<TypeReference>> = HashMap::new();
        let mut qualified_types: HashMap<String, Vec<QualifiedTypeImport>> = HashMap::new();
        for import in &imports {
            // An import with an `as` alias is qualified-only; without one it
            // also brings the names into unqualified scope.
            if import.alias.is_none() {
                for symbol in &import.symbols {
                    unqualified
                        .entry(symbol.local_name.clone())
                        .or_default()
                        .push(symbol.symbol);
                }
                for imported in &import.types {
                    imported_types
                        .entry(imported.name.clone())
                        .or_default()
                        .push(imported.reference);
                }
            }
            let qualifier = import
                .alias
                .clone()
                .unwrap_or_else(|| import.module_name.clone());
            let values = import
                .symbols
                .iter()
                .map(|symbol| (symbol.external_name.clone(), symbol.symbol))
                .collect();
            qualified
                .entry(qualifier.clone())
                .or_default()
                .push(QualifiedImport {
                    module: import.module,
                    values,
                });
            let types = import
                .types
                .iter()
                .map(|imported| (imported.name.clone(), imported.reference))
                .collect();
            qualified_types
                .entry(qualifier)
                .or_default()
                .push(QualifiedTypeImport {
                    module: import.module,
                    types,
                });
        }
        if !imports.iter().any(|import| import.module_name == "Prim") {
            for &(name, builtin) in &util::PRIM_TYPES {
                imported_types
                    .entry(name.to_owned())
                    .or_default()
                    .push(TypeReference::Builtin(builtin));
            }
        }
        let (fixities, type_fixities) = super::operators::merge_fixities(fixities, &imports);
        Self {
            globals,
            external_globals,
            type_names,
            opaque_types: HashSet::new(),
            imported_types,
            qualified_types,
            externals,
            imports,
            fixities,
            type_fixities,
            unqualified,
            qualified,
            export_items,
            scopes: Vec::new(),
            next_local: 0,
            errors,
            reported_conflicts: HashSet::new(),
        }
    }

    /// Registers a source-declared external (a `foreign import`) in the value
    /// namespace, reporting a duplicate against an existing external.
    pub(super) fn add_external(
        &mut self,
        name: String,
        symbol: SymbolId,
        external: ExternalSymbol,
        span: TextRange,
    ) {
        if self.external_globals.insert(name.clone(), symbol).is_some() {
            self.errors.push(ResolveError::named(
                ResolveErrorKind::DuplicateExternal,
                name,
                span,
            ));
        }
        self.externals.push(external);
    }

    pub(super) fn resolve_expr(&mut self, expression: ast::Expr) -> Option<Expr> {
        let span = expression.span;
        let kind = match expression.kind {
            AstExprKind::Name(name) => {
                if let Some(local) = self.lookup_local(&name.text) {
                    ExprKind::Local(local.id)
                } else {
                    ExprKind::Global(self.lookup_global(&name.text, name.span)?)
                }
            }
            AstExprKind::Integer(value) => ExprKind::Integer(value),
            AstExprKind::Number(value) => ExprKind::Number(value),
            AstExprKind::String(value) => ExprKind::String(value),
            AstExprKind::Char(value) => ExprKind::Char(value),
            AstExprKind::Array(elements) => ExprKind::Array(
                elements
                    .into_iter()
                    .map(|element| self.resolve_expr(element))
                    .collect::<Option<Vec<_>>>()?,
            ),
            AstExprKind::Record(fields) => ExprKind::Record(
                fields
                    .into_iter()
                    .map(|(label, value)| Some((label, self.resolve_expr(value)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            AstExprKind::MatchProduct(fields) => ExprKind::MatchProduct(
                fields
                    .into_iter()
                    .map(|(label, value)| Some((label, self.resolve_expr(value)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            AstExprKind::RecordUpdate { expression, fields } => {
                self.resolve_record_update(*expression, fields, span)?
            }
            AstExprKind::FieldAccess { expression, field } => ExprKind::FieldAccess {
                expression: Box::new(self.resolve_expr(*expression)?),
                field,
            },
            AstExprKind::Application(function, argument) => {
                let function = self.resolve_expr(*function);
                let argument = self.resolve_expr(*argument);
                ExprKind::Application(Box::new(function?), Box::new(argument?))
            }
            AstExprKind::Typed { expression, ty } => {
                let expression = self.resolve_expr(*expression);
                let ty = self.resolve_type(ty);
                ExprKind::Typed {
                    expression: Box::new(expression?),
                    ty: ty?,
                }
            }
            AstExprKind::TypeApplication { expression, ty } => {
                let expression = self.resolve_expr(*expression);
                let ty = self.resolve_type(ty);
                ExprKind::TypeApplication {
                    expression: Box::new(expression?),
                    ty: ty?,
                }
            }
            AstExprKind::Operator {
                operator,
                left,
                right,
            } => {
                let operator_span = operator.span;
                let symbol = self.lookup_global(&operator.text, operator_span);
                let left = self.resolve_expr(*left);
                let right = self.resolve_expr(*right);
                ExprKind::Operator {
                    operator: symbol?,
                    operator_span,
                    left: Box::new(left?),
                    right: Box::new(right?),
                }
            }
            AstExprKind::Negate {
                minus_span,
                expression,
            } => self.resolve_negate(minus_span, *expression)?,
            AstExprKind::OperatorChain {
                operands,
                operators,
            } => self.resolve_operator_chain(operands, operators)?,
            AstExprKind::OperatorSection {
                operator,
                operand,
                side,
            } => self.resolve_operator_section(operator, *operand, side, span)?,
            AstExprKind::Lambda { binder, body } => {
                let binder = self.new_local(binder.name, binder.span);
                self.scopes
                    .push(HashMap::from([(binder.name.clone(), binder.clone())]));
                let body = self.resolve_expr(*body);
                self.scopes.pop();
                ExprKind::Lambda {
                    binder,
                    body: Box::new(body?),
                }
            }
            AstExprKind::Let { declarations, body } => self.resolve_let(declarations, *body)?,
            AstExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.resolve_expr(*condition);
                let then_branch = self.resolve_expr(*then_branch);
                let else_branch = self.resolve_expr(*else_branch);
                ExprKind::If {
                    condition: Box::new(condition?),
                    then_branch: Box::new(then_branch?),
                    else_branch: Box::new(else_branch?),
                }
            }
            AstExprKind::Case {
                scrutinee,
                branches,
            } => {
                let scrutinee = self.resolve_expr(*scrutinee)?;
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    let mut scope = HashMap::new();
                    let pattern = self.resolve_pattern(branch.pattern, &mut scope)?;
                    let coverage = if ast_expr_is_guarded(&branch.value) {
                        CaseBranchCoverage::Guarded
                    } else {
                        CaseBranchCoverage::Source
                    };
                    self.scopes.push(scope);
                    let value = self.resolve_expr(branch.value);
                    self.scopes.pop();
                    lowered.push(hir::CaseBranch {
                        pattern,
                        value: value?,
                        span: branch.span,
                        coverage,
                    });
                }
                ExprKind::Case {
                    scrutinee: Box::new(scrutinee),
                    branches: lowered,
                }
            }
            AstExprKind::Guarded(clauses) => {
                ExprKind::Guarded(self.resolve_guarded_exprs(clauses)?)
            }
        };
        Some(Expr { kind, span })
    }

    fn resolve_record_update(
        &mut self,
        expression: ast::Expr,
        fields: Vec<ast::RecordUpdateField>,
        span: TextRange,
    ) -> Option<ExprKind> {
        let record = self.resolve_expr(expression)?;
        let binder = self.new_local("__psrs_record_update_base".to_owned(), record.span);
        let base = Expr {
            kind: ExprKind::Local(binder.id),
            span: record.span,
        };
        let fields = self.resolve_record_update_fields(&base, fields)?;
        let body = Expr {
            kind: ExprKind::RecordUpdate {
                expression: Box::new(base),
                fields,
            },
            span,
        };
        Some(ExprKind::Let {
            bindings: vec![LocalBinding {
                binder,
                value: record,
                span,
            }],
            body: Box::new(body),
        })
    }

    fn resolve_record_update_fields(
        &mut self,
        base: &Expr,
        fields: Vec<ast::RecordUpdateField>,
    ) -> Option<Vec<(String, Expr)>> {
        fields
            .into_iter()
            .map(|field| {
                let label = field.label;
                let value = match field.value {
                    ast::RecordUpdateValue::Nested {
                        fields,
                        span: value_span,
                    } => {
                        let nested_record = Expr {
                            kind: ExprKind::FieldAccess {
                                expression: Box::new(base.clone()),
                                field: label.clone(),
                            },
                            span: value_span,
                        };
                        let nested_fields =
                            self.resolve_record_update_fields(&nested_record, fields)?;
                        Expr {
                            kind: ExprKind::RecordUpdate {
                                expression: Box::new(nested_record),
                                fields: nested_fields,
                            },
                            span: value_span,
                        }
                    }
                    ast::RecordUpdateValue::Expression(value) => self.resolve_expr(value)?,
                };
                Some((label, value))
            })
            .collect()
    }

    pub(super) fn lookup_local(&self, name: &str) -> Option<&LocalBinder> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    pub(super) fn lookup_global(&mut self, text: &str, span: TextRange) -> Option<SymbolId> {
        if let Some((qualifier, member)) = split_qualified(text) {
            return self.lookup_qualified(text, qualifier, member, span);
        }
        if let Some(symbol) = self.globals.get(text) {
            return Some(*symbol);
        }
        if let Some(symbol) = self.external_globals.get(text) {
            return Some(*symbol);
        }
        if let Some(symbols) = self.unqualified.get(text) {
            let first = symbols[0];
            if symbols.iter().all(|symbol| *symbol == first) {
                return Some(first);
            }
            self.report_conflict(text.to_string(), span);
            return None;
        }
        self.report(ResolveErrorKind::UnknownName, text.to_string(), span);
        None
    }

    fn lookup_qualified(
        &mut self,
        text: &str,
        qualifier: &str,
        member: &str,
        span: TextRange,
    ) -> Option<SymbolId> {
        let Some(candidates) = self.qualified.get(qualifier) else {
            self.report(ResolveErrorKind::UnknownName, text.to_string(), span);
            return None;
        };
        let mut found: Option<(ModuleId, SymbolId)> = None;
        let mut conflict = false;
        for candidate in candidates {
            let Some(symbol) = candidate.values.get(member) else {
                continue;
            };
            match found {
                None => found = Some((candidate.module, *symbol)),
                Some((module, existing)) if module != candidate.module || existing != *symbol => {
                    conflict = true;
                }
                _ => {}
            }
        }
        if conflict {
            self.report_conflict(text.to_string(), span);
            return None;
        }
        if let Some((_, symbol)) = found {
            return Some(symbol);
        }
        self.report(ResolveErrorKind::UnknownName, text.to_string(), span);
        None
    }

    pub(super) fn new_local(&mut self, name: String, span: TextRange) -> LocalBinder {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        LocalBinder { id, name, span }
    }

    pub(super) fn report(&mut self, kind: ResolveErrorKind, name: String, span: TextRange) {
        self.errors.push(ResolveError::named(kind, name, span));
    }

    pub(super) fn report_conflict(&mut self, name: String, span: TextRange) {
        if self.reported_conflicts.insert(name.clone()) {
            self.errors.push(ResolveError::named(
                ResolveErrorKind::ScopeConflict,
                name,
                span,
            ));
        }
    }
}

mod guards;
mod negate;
mod util;

fn ast_expr_is_guarded(expression: &ast::Expr) -> bool {
    match &expression.kind {
        AstExprKind::Guarded(_) => true,
        AstExprKind::Let { body, .. } => ast_expr_is_guarded(body),
        _ => false,
    }
}

pub(super) use util::{
    PRIM_TYPES, builtin_type, implicit_prim_class, is_uppercase, prim_type, split_qualified,
};
