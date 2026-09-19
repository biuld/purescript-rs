use psrs_ast::{self as ast, ExprKind as AstExprKind};
use psrs_hir::{
    self as hir, BuiltinType, Declaration, Expr, ExprKind, ExternalKind, ExternalSymbol, Intrinsic,
    LocalBinder, LocalBinding, LocalId, ModuleId, RuntimeFunction, SymbolId, Type as HirType,
    TypeKind as HirTypeKind,
};
use psrs_span::TextRange;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveErrorKind {
    DuplicateDeclaration,
    DuplicateLocalBinding,
    DuplicateExternal,
    UnknownName,
    UnknownTypeName,
    InvalidHir,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveError {
    pub kind: ResolveErrorKind,
    pub span: TextRange,
    message: String,
}

impl ResolveError {
    fn named(kind: ResolveErrorKind, name: String, span: TextRange) -> Self {
        let message = match kind {
            ResolveErrorKind::DuplicateDeclaration => {
                format!("duplicate declaration `{name}`")
            }
            ResolveErrorKind::DuplicateLocalBinding => {
                format!("duplicate local binding `{name}`")
            }
            ResolveErrorKind::DuplicateExternal => {
                format!("duplicate external symbol `{name}`")
            }
            ResolveErrorKind::UnknownName => format!("unknown name `{name}`"),
            ResolveErrorKind::UnknownTypeName => format!("unknown type name `{name}`"),
            ResolveErrorKind::InvalidHir => format!("invalid resolved HIR: {name}"),
        };
        Self {
            kind,
            span,
            message,
        }
    }

    fn invalid_hir(span: TextRange, detail: &str) -> Self {
        Self {
            kind: ResolveErrorKind::InvalidHir,
            span,
            message: format!("resolved HIR invariant failed: {detail}"),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Resolves local and same-module value names in the currently supported AST.
/// Module IDs are assigned by the caller so a future module loader can own them.
pub fn resolve_module(
    module: ast::Module,
    module_id: ModuleId,
) -> Result<hir::Module, Vec<ResolveError>> {
    resolve_module_with_externals(module, module_id, &[])
}

/// Resolves a module using an explicit set of known external values.
pub fn resolve_module_with_externals(
    module: ast::Module,
    module_id: ModuleId,
    externals: &[ExternalSymbol],
) -> Result<hir::Module, Vec<ResolveError>> {
    let mut globals = HashMap::new();
    let mut external_globals = HashMap::new();
    let mut errors = Vec::new();

    for (index, declaration) in module.declarations.iter().enumerate() {
        let symbol = SymbolId::new(module_id, symbol_index(index));
        if globals
            .insert(declaration.name.text.clone(), symbol)
            .is_some()
        {
            errors.push(ResolveError::named(
                ResolveErrorKind::DuplicateDeclaration,
                declaration.name.text.clone(),
                declaration.name.span,
            ));
        }
    }

    for external in externals {
        if external_globals
            .insert(external.name.clone(), external.symbol)
            .is_some()
        {
            errors.push(ResolveError::named(
                ResolveErrorKind::DuplicateExternal,
                external.name.clone(),
                module.span,
            ));
        }
    }

    let mut resolver = Resolver {
        globals,
        external_globals,
        externals: externals.to_vec(),
        scopes: Vec::new(),
        next_local: 0,
        errors,
    };
    let declarations = module
        .declarations
        .into_iter()
        .enumerate()
        .filter_map(|(index, declaration)| {
            let value = resolver.resolve_expr(declaration.value)?;
            let signature = match declaration.annotation {
                Some(annotation) => Some(resolver.resolve_type(annotation)?),
                None => None,
            };
            Some(Declaration {
                symbol: SymbolId::new(module_id, symbol_index(index)),
                name: declaration.name.text,
                name_span: declaration.name.span,
                value,
                signature,
                span: declaration.span,
            })
        })
        .collect();

    if resolver.errors.is_empty() {
        let resolved = hir::Module {
            id: module_id,
            name: module.name.text,
            externals: resolver.externals,
            declarations,
            span: module.span,
        };
        match resolved.verify() {
            Ok(()) => Ok(resolved),
            Err(invariant_errors) => Err(invariant_errors
                .into_iter()
                .map(|error| ResolveError::invalid_hir(error.span, error.message))
                .collect()),
        }
    } else {
        Err(resolver.errors)
    }
}

struct Resolver {
    globals: HashMap<String, SymbolId>,
    external_globals: HashMap<String, SymbolId>,
    externals: Vec<ExternalSymbol>,
    scopes: Vec<HashMap<String, LocalBinder>>,
    next_local: u32,
    errors: Vec<ResolveError>,
}

impl Resolver {
    fn resolve_expr(&mut self, expression: ast::Expr) -> Option<Expr> {
        let span = expression.span;
        let kind = match expression.kind {
            AstExprKind::Name(name) => {
                if let Some(local) = self.lookup_local(&name.text) {
                    ExprKind::Local(local.id)
                } else if let Some(symbol) = self.globals.get(&name.text) {
                    ExprKind::Global(*symbol)
                } else if let Some(symbol) = self.external_globals.get(&name.text) {
                    ExprKind::Global(*symbol)
                } else {
                    self.report(ResolveErrorKind::UnknownName, name.text, name.span);
                    return None;
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
                let symbol = self
                    .globals
                    .get(&operator.text)
                    .or_else(|| self.external_globals.get(&operator.text))
                    .copied();
                if symbol.is_none() {
                    self.report(
                        ResolveErrorKind::UnknownName,
                        operator.text.clone(),
                        operator_span,
                    );
                }
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
        };
        Some(Expr { kind, span })
    }

    fn resolve_type(&mut self, expression: ast::Type) -> Option<HirType> {
        let span = expression.span;
        let kind = match expression.kind {
            ast::TypeKind::Name(name) => match name.text.as_str() {
                "Int" => HirTypeKind::Constructor(BuiltinType::Int),
                "Boolean" => HirTypeKind::Constructor(BuiltinType::Boolean),
                "String" => HirTypeKind::Constructor(BuiltinType::String),
                "Unit" => HirTypeKind::Constructor(BuiltinType::Unit),
                _ if name
                    .text
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_uppercase()) =>
                {
                    self.report(ResolveErrorKind::UnknownTypeName, name.text, name.span);
                    return None;
                }
                _ => HirTypeKind::Variable(name.text),
            },
            ast::TypeKind::Function { parameter, result } => HirTypeKind::Function {
                parameter: Box::new(self.resolve_type(*parameter)?),
                result: Box::new(self.resolve_type(*result)?),
            },
            ast::TypeKind::Forall { body, .. } => self.resolve_type(*body)?.kind,
        };
        Some(HirType { kind, span })
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

    fn new_local(&mut self, name: String, span: TextRange) -> LocalBinder {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        LocalBinder { id, name, span }
    }

    fn report(&mut self, kind: ResolveErrorKind, name: String, span: TextRange) {
        self.errors.push(ResolveError::named(kind, name, span));
    }
}

fn symbol_index(index: usize) -> u32 {
    u32::try_from(index).expect("a source module cannot contain more declarations than its range")
}

/// The compiler-known externals available to every bootstrap module.
pub fn bootstrap_externals() -> Vec<ExternalSymbol> {
    let intrinsics = [
        ("true", Intrinsic::BoolTrue),
        ("false", Intrinsic::BoolFalse),
        ("+", Intrinsic::I32Add),
        ("-", Intrinsic::I32Sub),
        ("*", Intrinsic::I32Mul),
        ("/", Intrinsic::I32DivS),
        ("%", Intrinsic::I32RemS),
        ("==", Intrinsic::I32Eq),
        ("/=", Intrinsic::I32Ne),
        ("<", Intrinsic::I32LtS),
        ("<=", Intrinsic::I32LeS),
        (">", Intrinsic::I32GtS),
        (">=", Intrinsic::I32GeS),
    ]
    .into_iter()
    .map(|(name, intrinsic)| ExternalSymbol {
        symbol: intrinsic.symbol(),
        name: name.into(),
        kind: ExternalKind::Intrinsic(intrinsic),
    });
    let runtime = [("log", RuntimeFunction::ConsoleLog)]
        .into_iter()
        .map(|(name, function)| ExternalSymbol {
            symbol: function.symbol(),
            name: name.into(),
            kind: ExternalKind::Runtime(function),
        });
    intrinsics.chain(runtime).collect()
}

#[cfg(test)]
mod tests;
