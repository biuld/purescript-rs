use super::{ResolveError, ResolveErrorKind};
use psrs_ast::{self as ast, ExprKind as AstExprKind};
use psrs_hir::{
    self as hir, BuiltinType, Expr, ExprKind, ExternalSymbol, LocalBinder, LocalBinding, LocalId,
    ModuleId, SymbolId, TypeId,
};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

struct QualifiedImport {
    module: ModuleId,
    values: HashMap<String, SymbolId>,
}

pub(super) struct QualifiedTypeImport {
    pub(super) module: ModuleId,
    pub(super) types: HashMap<String, TypeId>,
}

pub(super) struct Resolver {
    pub(super) globals: HashMap<String, SymbolId>,
    external_globals: HashMap<String, SymbolId>,
    pub(super) type_names: HashMap<String, TypeId>,
    pub(super) imported_types: HashMap<String, Vec<TypeId>>,
    pub(super) qualified_types: HashMap<String, Vec<QualifiedTypeImport>>,
    pub(super) externals: Vec<ExternalSymbol>,
    pub(super) imports: Vec<hir::Import>,
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
        errors: Vec<ResolveError>,
    ) -> Self {
        let mut unqualified: HashMap<String, Vec<SymbolId>> = HashMap::new();
        let mut qualified: HashMap<String, Vec<QualifiedImport>> = HashMap::new();
        let mut imported_types: HashMap<String, Vec<TypeId>> = HashMap::new();
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
                        .push(imported.id);
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
                .map(|imported| (imported.name.clone(), imported.id))
                .collect();
            qualified_types
                .entry(qualifier)
                .or_default()
                .push(QualifiedTypeImport {
                    module: import.module,
                    types,
                });
        }
        Self {
            globals,
            external_globals,
            type_names,
            imported_types,
            qualified_types,
            externals,
            imports,
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
            AstExprKind::String(value) => ExprKind::String(value),
            AstExprKind::Char(value) => ExprKind::Char(value),
            AstExprKind::Application(function, argument) => {
                let function = self.resolve_expr(*function);
                let argument = self.resolve_expr(*argument);
                ExprKind::Application(Box::new(function?), Box::new(argument?))
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
                    self.scopes.push(scope);
                    let value = self.resolve_expr(branch.value);
                    self.scopes.pop();
                    lowered.push(hir::CaseBranch {
                        pattern,
                        value: value?,
                        span: branch.span,
                    });
                }
                ExprKind::Case {
                    scrutinee: Box::new(scrutinee),
                    branches: lowered,
                }
            }
        };
        Some(Expr { kind, span })
    }

    fn resolve_pattern(
        &mut self,
        pattern: ast::Pattern,
        scope: &mut HashMap<String, LocalBinder>,
    ) -> Option<hir::Pattern> {
        let span = pattern.span;
        let kind = match pattern.kind {
            ast::PatternKind::Wildcard => hir::PatternKind::Wildcard,
            ast::PatternKind::Var(binder) => {
                let binder = self.new_local(binder.name, binder.span);
                scope.insert(binder.name.clone(), binder.clone());
                hir::PatternKind::Var(binder)
            }
            ast::PatternKind::Constructor { name, arguments } => {
                let symbol = self.lookup_global(&name.text, name.span)?;
                hir::PatternKind::Constructor {
                    symbol,
                    name_span: name.span,
                    arguments: arguments
                        .into_iter()
                        .map(|argument| self.resolve_pattern(argument, scope))
                        .collect::<Option<Vec<_>>>()?,
                }
            }
        };
        Some(hir::Pattern { kind, span })
    }

    fn resolve_let(
        &mut self,
        declarations: Vec<ast::Declaration>,
        body: ast::Expr,
    ) -> Option<ExprKind> {
        let mut scope = HashMap::new();
        let mut binders = Vec::with_capacity(declarations.len());
        for declaration in &declarations {
            let binder = self.new_local(declaration.name.text.clone(), declaration.name.span);
            if scope.insert(binder.name.clone(), binder.clone()).is_some() {
                self.report(
                    ResolveErrorKind::DuplicateLocalBinding,
                    binder.name.clone(),
                    binder.span,
                );
            }
            binders.push(binder);
        }

        self.scopes.push(scope);
        let bindings = declarations
            .into_iter()
            .zip(binders)
            .filter_map(|(declaration, binder)| {
                let value = self.resolve_expr(declaration.value)?;
                Some(LocalBinding {
                    binder,
                    value,
                    span: declaration.span,
                })
            })
            .collect::<Vec<_>>();
        let body = self.resolve_expr(body);
        self.scopes.pop();
        Some(ExprKind::Let {
            bindings,
            body: Box::new(body?),
        })
    }

    fn lookup_local(&self, name: &str) -> Option<&LocalBinder> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn lookup_global(&mut self, text: &str, span: TextRange) -> Option<SymbolId> {
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
        let candidates = self.qualified.get(qualifier)?;
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

    fn new_local(&mut self, name: String, span: TextRange) -> LocalBinder {
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

pub(super) fn split_qualified(text: &str) -> Option<(&str, &str)> {
    if let Some(index) = text.rfind(".(")
        && text.ends_with(')')
    {
        return Some((&text[..index], &text[index + 2..text.len() - 1]));
    }
    let index = text.rfind('.')?;
    Some((&text[..index], &text[index + 1..]))
}

pub(super) fn is_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_uppercase())
}

pub(super) fn builtin_type(name: &str) -> Option<BuiltinType> {
    Some(match name {
        "Int" => BuiltinType::Int,
        "Boolean" => BuiltinType::Boolean,
        "String" => BuiltinType::String,
        "Unit" => BuiltinType::Unit,
        "Type" => BuiltinType::Type,
        "Constraint" => BuiltinType::Constraint,
        "Symbol" => BuiltinType::Symbol,
        "Row" => BuiltinType::Row,
        "Record" => BuiltinType::Record,
        "Array" => BuiltinType::Array,
        "Function" | "->" | "~>" => BuiltinType::Function,
        _ => return None,
    })
}
