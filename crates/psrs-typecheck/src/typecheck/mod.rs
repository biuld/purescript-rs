use psrs_hir::{self as hir, Intrinsic, LocalBinder, LocalId, SymbolId};
use psrs_span::TextRange;
use psrs_thir::{self as thir, Type, TypeId};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheckErrorKind {
    InvalidHir,
    TypeMismatch,
    OccursCheck,
    UnconstrainedType,
    IntegerOutOfRange,
    UnsupportedExpression,
    UnsupportedIntrinsic,
    UnloweredOperator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeCheckError {
    pub kind: TypeCheckErrorKind,
    pub span: TextRange,
    message: String,
}

impl TypeCheckError {
    fn new(kind: TypeCheckErrorKind, span: TextRange, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn typecheck_module(module: hir::Module) -> Result<thir::Module, Vec<TypeCheckError>> {
    if let Err(errors) = module.verify() {
        return Err(errors
            .into_iter()
            .map(|error| {
                TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    error.span,
                    format!("invalid HIR: {}", error.message),
                )
            })
            .collect());
    }

    let mut checker = Checker::new(&module);
    let declarations = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let value = checker.infer_expr(&declaration.value)?;
            let ty = checker.globals[&declaration.symbol].clone();
            checker.unify(ty.clone(), value.ty.clone(), declaration.span);
            Some(InferredDeclaration {
                symbol: declaration.symbol,
                name: declaration.name.clone(),
                name_span: declaration.name_span,
                ty,
                value,
                span: declaration.span,
            })
        })
        .collect::<Vec<_>>();

    if !checker.errors.is_empty() {
        return Err(checker.errors);
    }

    let mut types = TypeInterner::default();
    let declarations = declarations
        .into_iter()
        .filter_map(|declaration| {
            let ty = checker.finalize_type(&declaration.ty, declaration.name_span, &mut types)?;
            let value = checker.finalize_expr(declaration.value, &mut types)?;
            Some(thir::Declaration {
                symbol: declaration.symbol,
                name: declaration.name,
                name_span: declaration.name_span,
                ty,
                value,
                span: declaration.span,
            })
        })
        .collect::<Vec<_>>();
    if !checker.errors.is_empty() {
        return Err(checker.errors);
    }

    let typed = thir::Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types: types.values,
        declarations,
        span: module.span,
    };
    match typed.verify() {
        Ok(()) => Ok(typed),
        Err(errors) => Err(errors
            .into_iter()
            .map(|error| {
                TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    error.span,
                    format!("invalid THIR: {}", error.message),
                )
            })
            .collect()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InferType {
    Variable(u32),
    I32,
    Boolean,
    Function(Box<InferType>, Box<InferType>),
}

#[derive(Clone, Debug)]
struct InferredDeclaration {
    symbol: SymbolId,
    name: String,
    name_span: TextRange,
    ty: InferType,
    value: InferredExpr,
    span: TextRange,
}

#[derive(Clone, Debug)]
struct InferredBinder {
    binder: LocalBinder,
    ty: InferType,
}

#[derive(Clone, Debug)]
struct InferredBinding {
    binder: InferredBinder,
    value: InferredExpr,
    span: TextRange,
}

#[derive(Clone, Debug)]
struct InferredExpr {
    kind: InferredExprKind,
    ty: InferType,
    span: TextRange,
}

#[derive(Clone, Debug)]
enum InferredExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(i32),
    Boolean(bool),
    Application(Box<InferredExpr>, Box<InferredExpr>),
    Lambda {
        binder: InferredBinder,
        body: Box<InferredExpr>,
    },
    Let {
        bindings: Vec<InferredBinding>,
        body: Box<InferredExpr>,
    },
    If {
        condition: Box<InferredExpr>,
        then_branch: Box<InferredExpr>,
        else_branch: Box<InferredExpr>,
    },
}

struct Checker {
    globals: HashMap<SymbolId, InferType>,
    external_intrinsics: HashMap<SymbolId, Intrinsic>,
    locals: HashMap<LocalId, InferType>,
    substitutions: HashMap<u32, InferType>,
    next_variable: u32,
    errors: Vec<TypeCheckError>,
}
#[derive(Default)]
struct TypeInterner {
    values: Vec<Type>,
    ids: HashMap<Type, TypeId>,
}

impl TypeInterner {
    fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(id) = self.ids.get(&ty) {
            return *id;
        }
        let id = TypeId(self.values.len() as u32);
        self.values.push(ty.clone());
        self.ids.insert(ty, id);
        id
    }
}

fn occurs(variable: u32, ty: &InferType) -> bool {
    match ty {
        InferType::Variable(other) => variable == *other,
        InferType::Function(parameter, result) => {
            occurs(variable, parameter) || occurs(variable, result)
        }
        InferType::I32 | InferType::Boolean => false,
    }
}

impl std::fmt::Display for InferType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Variable(variable) => write!(f, "_T{variable}"),
            Self::I32 => f.write_str("Int"),
            Self::Boolean => f.write_str("Boolean"),
            Self::Function(parameter, result) => write!(f, "({parameter} -> {result})"),
        }
    }
}

#[cfg(test)]
mod tests;

mod infer;
