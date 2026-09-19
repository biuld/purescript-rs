use psrs_hir::{
    self as hir, ExternalKind, Intrinsic, LocalBinder, LocalId, RuntimeFunction, SymbolId,
    TypeVariableId,
};
use psrs_span::TextRange;
use psrs_thir::{self as thir, Type, TypeId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheckErrorKind {
    InvalidHir,
    TypeMismatch,
    OccursCheck,
    UnconstrainedType,
    IntegerOutOfRange,
    UnsupportedExpression,
    UnsupportedType,
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
    let components = order::declaration_order(&module);
    let mut inferred = (0..module.declarations.len())
        .map(|_| None)
        .collect::<Vec<Option<InferredDeclaration>>>();

    for component in &components {
        for &index in component {
            let declaration = &module.declarations[index];
            let ty = match &declaration.signature {
                Some(signature) => checker.elaborate_signature(signature),
                None => checker.fresh(),
            };
            checker
                .globals
                .insert(declaration.symbol, Scheme::monomorphic(ty));
        }
        for &index in component {
            let declaration = &module.declarations[index];
            let Some(value) = checker.infer_expr(&declaration.value) else {
                continue;
            };
            let scheme = checker.globals[&declaration.symbol].clone();
            let span = declaration
                .signature
                .as_ref()
                .map_or(declaration.name_span, |signature| signature.span);
            checker.unify(scheme.ty.clone(), value.ty.clone(), span);
            inferred[index] = Some(InferredDeclaration {
                symbol: declaration.symbol,
                name: declaration.name.clone(),
                name_span: declaration.name_span,
                scheme,
                value,
                span: declaration.span,
            });
        }
        // Generalize after the component is inferred so later components
        // instantiate polymorphic definitions.
        for &index in component {
            let Some(monomorphic) = inferred[index].as_ref().map(|d| d.scheme.ty.clone()) else {
                continue;
            };
            let scheme = checker.generalize(&monomorphic, TOP_LEVEL);
            if let Some(declaration) = inferred[index].as_mut() {
                declaration.scheme = scheme.clone();
                checker.globals.insert(declaration.symbol, scheme);
            }
        }
    }

    if !checker.errors.is_empty() {
        return Err(checker.errors);
    }
    let inferred = inferred.into_iter().flatten().collect::<Vec<_>>();

    let mut types = TypeInterner::default();
    let generics = checker.generic_variables.clone();
    let declarations = inferred
        .into_iter()
        .filter_map(|declaration| {
            let quantified = declaration
                .scheme
                .variables
                .iter()
                .copied()
                .map(TypeVariableId)
                .collect();
            let ty = checker.finalize_type(
                &declaration.scheme.ty,
                declaration.name_span,
                &mut types,
                &generics,
            )?;
            let value = checker.finalize_expr(declaration.value, &mut types, &generics)?;
            Some(thir::Declaration {
                symbol: declaration.symbol,
                name: declaration.name,
                name_span: declaration.name_span,
                quantified,
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

/// The level of the empty top-level environment. All declaration variables are
/// created at a higher level, so top-level generalization quantifies them.
const TOP_LEVEL: u32 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum InferType {
    Variable(u32),
    I32,
    Boolean,
    String,
    Unit,
    Constructor(TypeConstructor),
    Application(Box<InferType>, Box<InferType>),
    Function(Box<InferType>, Box<InferType>),
}

/// A type constructor during inference. `Array` is the only built-in the
/// current front end elaborates; user constructors keep their resolved HIR ID so
/// distinct declarations never unify by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TypeConstructor {
    Array,
    User(hir::TypeId),
}

/// A type with a set of universally quantified variables.
#[derive(Clone, Debug)]
struct Scheme {
    variables: Vec<u32>,
    ty: InferType,
}

impl Scheme {
    fn monomorphic(ty: InferType) -> Self {
        Self {
            variables: Vec::new(),
            ty,
        }
    }
}

#[derive(Clone, Debug)]
struct InferredDeclaration {
    symbol: SymbolId,
    name: String,
    name_span: TextRange,
    scheme: Scheme,
    value: InferredExpr,
    span: TextRange,
}

#[derive(Clone, Debug)]
struct InferredBinder {
    binder: LocalBinder,
    scheme: Scheme,
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
    String(String),
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
    globals: HashMap<SymbolId, Scheme>,
    external_kinds: HashMap<SymbolId, ExternalKind>,
    locals: HashMap<LocalId, Scheme>,
    type_names: HashMap<hir::TypeId, String>,
    substitutions: HashMap<u32, InferType>,
    levels: HashMap<u32, u32>,
    generic_variables: HashSet<u32>,
    rigid: HashSet<u32>,
    next_variable: u32,
    level: u32,
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
        InferType::Application(function, argument) | InferType::Function(function, argument) => {
            occurs(variable, function) || occurs(variable, argument)
        }
        InferType::I32
        | InferType::Boolean
        | InferType::String
        | InferType::Unit
        | InferType::Constructor(_) => false,
    }
}

#[cfg(test)]
mod tests;

mod finalize;
mod infer;
mod order;
mod signature;
mod unify;
