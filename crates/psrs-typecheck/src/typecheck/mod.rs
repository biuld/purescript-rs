use psrs_hir::{
    self as hir, ExternalKind, Intrinsic, LocalBinder, LocalId, SymbolId, TypeVariableId,
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
    NumberOutOfRange,
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
    typecheck_module_with_imports(module, &HashMap::new())
}

/// Type checks a module against the declared types of values it imports from
/// other modules. Imported symbols resolve to their exporting declaration's
/// signature, which the caller reads from the exporting module's HIR.
pub fn typecheck_module_with_imports(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_imports_and_effect_representation(module, imported, false)
}

/// Type checks a trusted embedded library module whose `Effect a` values are
/// implemented as token-taking closures. Ordinary source modules must use
/// [`typecheck_module_with_imports`] so `Effect` remains abstract while
/// unification runs.
pub fn typecheck_module_with_imports_and_effect_representation(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_runtime_representation: bool,
) -> Result<thir::Module, Vec<TypeCheckError>> {
    typecheck_module_with_imports_and_effect_context(
        module,
        imported,
        None,
        effect_runtime_representation,
    )
}

/// Type checks a module with the resolved identity of the library's abstract
/// `Effect` type, including when that identity arrives through transitive value
/// signatures.
pub fn typecheck_module_with_imports_and_effect_context(
    module: hir::Module,
    imported: &HashMap<SymbolId, hir::Type>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
) -> Result<thir::Module, Vec<TypeCheckError>> {
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

    let mut checker = Checker::new(
        &module,
        imported,
        effect_type,
        effect_runtime_representation,
    );
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
            let expected = declaration
                .signature
                .as_ref()
                .map(|_| checker.globals[&declaration.symbol].ty.clone());
            let Some(value) = checker.infer_expr_with_expected(&declaration.value, expected) else {
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
    let mut generics = checker.generic_variables.clone();
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

    let constructor_infos = checker
        .constructor_info
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let newtype_ids = module
        .types
        .iter()
        .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::Newtype)
        .map(|declaration| declaration.id)
        .collect();
    let mut constructors = Vec::with_capacity(constructor_infos.len());
    for info in constructor_infos {
        let mut variables = HashMap::new();
        for parameter in &info.parameters {
            let variable = checker.fresh();
            if let InferType::Variable(id) = variable {
                generics.insert(id);
            }
            variables.insert(parameter.clone(), variable);
        }
        let Some(field_types) = info
            .fields
            .iter()
            .map(|field| {
                let inferred = checker.elaborate_type(field, &mut variables);
                checker.finalize_type(&inferred, field.span, &mut types, &generics)
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        constructors.push(thir::ConstructorInfo {
            symbol: info.symbol,
            name: info.name.clone(),
            type_id: info.type_id,
            tag: info.tag,
            field_count: field_types.len(),
            field_types,
        });
    }

    let typed = thir::Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types: types.values,
        newtype_ids,
        constructors,
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
    F64,
    Boolean,
    String,
    Char,
    Unit,
    Constructor(TypeConstructor),
    Application(Box<InferType>, Box<InferType>),
    Record(Vec<(String, InferType)>),
    Function(Box<InferType>, Box<InferType>),
}

/// A type constructor during inference. `Array` is the only built-in the
/// current front end elaborates; user constructors keep their resolved HIR ID so
/// distinct declarations never unify by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TypeConstructor {
    Array,
    Effect,
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
    Number(String),
    Boolean(bool),
    String(String),
    Char(char),
    Array(Vec<InferredExpr>),
    Record(Vec<(String, InferredExpr)>),
    RecordUpdate {
        expression: Box<InferredExpr>,
        fields: Vec<(String, InferredExpr)>,
    },
    FieldAccess {
        expression: Box<InferredExpr>,
        field: String,
    },
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
    Case {
        scrutinee: Box<InferredExpr>,
        branches: Vec<InferredCaseBranch>,
    },
}

#[derive(Clone, Debug)]
struct InferredCaseBranch {
    pattern: InferredPattern,
    value: InferredExpr,
    span: TextRange,
}

#[derive(Clone, Debug)]
struct InferredPattern {
    kind: InferredPatternKind,
    ty: InferType,
    span: TextRange,
}

#[derive(Clone, Debug)]
enum InferredPatternKind {
    Wildcard,
    Var {
        binder: LocalBinder,
        ty: InferType,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<InferredPattern>,
    },
    Record {
        fields: Vec<(String, InferredPattern)>,
    },
}

/// A resolved type synonym, expanded during signature elaboration.
#[derive(Clone, Debug)]
struct Synonym {
    parameters: Vec<String>,
    body: hir::Type,
}

/// A data or newtype constructor registered as a value, with its declared
/// result type and field types.
#[derive(Clone, Debug)]
struct ConstructorInfo {
    symbol: SymbolId,
    name: String,
    type_id: hir::TypeId,
    tag: u32,
    parameters: Vec<String>,
    fields: Vec<hir::Type>,
}

struct Checker {
    globals: HashMap<SymbolId, Scheme>,
    external_kinds: HashMap<SymbolId, ExternalKind>,
    external_signatures: HashMap<SymbolId, hir::Type>,
    /// Declared types of values imported from other modules, keyed by the
    /// exporting declaration's symbol.
    imported: HashMap<SymbolId, hir::Type>,
    locals: HashMap<LocalId, Scheme>,
    type_names: HashMap<hir::TypeId, String>,
    synonyms: HashMap<hir::TypeId, Synonym>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
    constructor_info: HashMap<SymbolId, ConstructorInfo>,
    expanding: HashSet<hir::TypeId>,
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
        InferType::Record(fields) => fields.iter().any(|(_, field)| occurs(variable, field)),
        InferType::I32
        | InferType::F64
        | InferType::Boolean
        | InferType::String
        | InferType::Char
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
