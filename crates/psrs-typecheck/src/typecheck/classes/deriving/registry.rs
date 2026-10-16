//! The single table of compiler-known deriving classes and the representation
//! declarations their rules name.
//!
//! The table is built once from the resolved declarations and read by
//! `TypeId`; no deriving rule compares a class module or name at use time. The
//! declaring module and short name are inputs to construction only, so a
//! re-export keeps the defining identity and an unrelated same-name class gains
//! no rule. See [deriving](../../../../docs/design/frontend/type-system/deriving.md).

use super::super::super::*;

/// A class with a compiler-supported deriving rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::typecheck) enum KnownClass {
    Eq,
    Eq1,
    Ord,
    Ord1,
    Functor,
    Bifunctor,
    Contravariant,
    Profunctor,
    Foldable,
    Bifoldable,
    Traversable,
    Bitraversable,
    Newtype,
    Generic,
}

const ALL_KNOWN_CLASSES: [KnownClass; 14] = [
    KnownClass::Eq,
    KnownClass::Eq1,
    KnownClass::Ord,
    KnownClass::Ord1,
    KnownClass::Functor,
    KnownClass::Bifunctor,
    KnownClass::Contravariant,
    KnownClass::Profunctor,
    KnownClass::Foldable,
    KnownClass::Bifoldable,
    KnownClass::Traversable,
    KnownClass::Bitraversable,
    KnownClass::Newtype,
    KnownClass::Generic,
];

impl KnownClass {
    /// The declaring module and class name this rule is pinned to. This is a
    /// construction-time key, not a use-time predicate.
    pub(in crate::typecheck) fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::Eq => ("Data.Eq", "Eq"),
            Self::Eq1 => ("Data.Eq", "Eq1"),
            Self::Ord => ("Data.Ord", "Ord"),
            Self::Ord1 => ("Data.Ord", "Ord1"),
            Self::Functor => ("Data.Functor", "Functor"),
            Self::Bifunctor => ("Data.Bifunctor", "Bifunctor"),
            Self::Contravariant => ("Data.Functor.Contravariant", "Contravariant"),
            Self::Profunctor => ("Data.Profunctor", "Profunctor"),
            Self::Foldable => ("Data.Foldable", "Foldable"),
            Self::Bifoldable => ("Data.Bifoldable", "Bifoldable"),
            Self::Traversable => ("Data.Traversable", "Traversable"),
            Self::Bitraversable => ("Data.Bitraversable", "Bitraversable"),
            Self::Newtype => ("Data.Newtype", "Newtype"),
            Self::Generic => ("Data.Generic.Rep", "Generic"),
        }
    }

    /// Whether the rule only fixes the instance head's final type argument
    /// (Newtype, Generic) rather than deriving method bodies from fields.
    pub(in crate::typecheck) fn is_head_shape(self) -> bool {
        matches!(self, Self::Newtype | Self::Generic)
    }
}

/// The `Data.Ordering` constructors an `Ord` rule names.
#[derive(Clone, Copy, Debug)]
pub(in crate::typecheck) struct OrderingRep {
    pub(in crate::typecheck) lt: SymbolId,
    pub(in crate::typecheck) eq: SymbolId,
    pub(in crate::typecheck) gt: SymbolId,
}

/// The `Data.Generic.Rep` types and constructors a `Generic` rule names.
#[derive(Clone, Copy, Debug)]
pub(in crate::typecheck) struct GenericRep {
    constructor_ty: hir::TypeId,
    no_constructors: hir::TypeId,
    no_arguments: hir::TypeId,
    argument: hir::TypeId,
    product: hir::TypeId,
    sum: hir::TypeId,
    constructor_ctor: SymbolId,
    no_arguments_ctor: SymbolId,
    argument_ctor: SymbolId,
    product_ctor: SymbolId,
    inl_ctor: SymbolId,
    inr_ctor: SymbolId,
}

impl GenericRep {
    /// The representation type named `name`.
    pub(in crate::typecheck) fn type_id(&self, name: &str) -> Option<hir::TypeId> {
        match name {
            "Constructor" => Some(self.constructor_ty),
            "NoConstructors" => Some(self.no_constructors),
            "NoArguments" => Some(self.no_arguments),
            "Argument" => Some(self.argument),
            "Product" => Some(self.product),
            "Sum" => Some(self.sum),
            _ => None,
        }
    }

    /// The representation constructor named `name`. `Inl` and `Inr` are the
    /// `Sum` constructors.
    pub(in crate::typecheck) fn constructor(&self, name: &str) -> Option<SymbolId> {
        match name {
            "Constructor" => Some(self.constructor_ctor),
            "NoArguments" => Some(self.no_arguments_ctor),
            "Argument" => Some(self.argument_ctor),
            "Product" => Some(self.product_ctor),
            "Inl" => Some(self.inl_ctor),
            "Inr" => Some(self.inr_ctor),
            _ => None,
        }
    }
}

impl OrderingRep {
    fn build(env: &SemanticEnv) -> Option<Self> {
        let (_, declaration) = find_std_type(env, "Data.Ordering", "Ordering")?;
        Some(Self {
            lt: nullary_constructor(declaration, "LT")?,
            eq: nullary_constructor(declaration, "EQ")?,
            gt: nullary_constructor(declaration, "GT")?,
        })
    }
}

impl GenericRep {
    fn build(env: &SemanticEnv) -> Option<Self> {
        let (constructor_ty, constructor_declaration) =
            find_std_type(env, "Data.Generic.Rep", "Constructor")?;
        let (no_constructors, _) = find_std_type(env, "Data.Generic.Rep", "NoConstructors")?;
        let (no_arguments, no_arguments_declaration) =
            find_std_type(env, "Data.Generic.Rep", "NoArguments")?;
        let (argument, argument_declaration) = find_std_type(env, "Data.Generic.Rep", "Argument")?;
        let (product, product_declaration) = find_std_type(env, "Data.Generic.Rep", "Product")?;
        let (sum, sum_declaration) = find_std_type(env, "Data.Generic.Rep", "Sum")?;
        Some(Self {
            constructor_ty,
            no_constructors,
            no_arguments,
            argument,
            product,
            sum,
            constructor_ctor: constructor_symbol(constructor_declaration, "Constructor")?,
            no_arguments_ctor: constructor_symbol(no_arguments_declaration, "NoArguments")?,
            argument_ctor: constructor_symbol(argument_declaration, "Argument")?,
            product_ctor: constructor_symbol(product_declaration, "Product")?,
            inl_ctor: constructor_symbol(sum_declaration, "Inl")?,
            inr_ctor: constructor_symbol(sum_declaration, "Inr")?,
        })
    }
}

/// The single source of "which classes the compiler knows" and the symbols
/// their rules name. Built once when the semantic environment is constructed.
#[derive(Clone, Debug, Default)]
pub(in crate::typecheck) struct DerivingRegistry {
    classes: HashMap<hir::TypeId, KnownClass>,
    ids: HashMap<KnownClass, hir::TypeId>,
    methods: HashMap<(KnownClass, String), SymbolId>,
    values: HashMap<(String, String), SymbolId>,
    ordering: Option<OrderingRep>,
    generic: Option<GenericRep>,
}

impl DerivingRegistry {
    pub(in crate::typecheck) fn build(
        env: &SemanticEnv,
        known_values: &[hir::Declaration],
        module_names: &HashMap<hir::ModuleId, String>,
    ) -> Self {
        let mut registry = Self::default();
        for (class_id, class) in &env.classes {
            let Some(module) = env.type_modules.get(class_id).map(String::as_str) else {
                continue;
            };
            let Some(name) = env.type_names.get(class_id).map(String::as_str) else {
                continue;
            };
            let Some(known) = ALL_KNOWN_CLASSES
                .into_iter()
                .find(|known| known.identity() == (module, name))
            else {
                continue;
            };
            registry.classes.insert(*class_id, known);
            registry.ids.insert(known, *class_id);
            for method in &class.methods {
                registry
                    .methods
                    .insert((known, method.name.clone()), method.symbol);
            }
        }
        // A class method is a value too: `append`, `mempty`, `apply`, and
        // `pure` are methods of classes that are not deriving classes, so the
        // fold and traversal rules reach them through the value table.
        for (class_id, class) in &env.classes {
            let Some(module) = env.type_modules.get(class_id) else {
                continue;
            };
            for method in &class.methods {
                registry
                    .values
                    .insert((module.clone(), method.name.clone()), method.symbol);
            }
        }
        for declaration in known_values {
            if let Some(module) = module_names.get(&declaration.symbol.module) {
                registry.values.insert(
                    (module.clone(), declaration.name.clone()),
                    declaration.symbol,
                );
            }
        }
        registry.ordering = OrderingRep::build(env);
        registry.generic = GenericRep::build(env);
        registry
    }

    /// The deriving rule for a resolved class identity, if the compiler knows
    /// one.
    pub(in crate::typecheck) fn known_class(&self, class_id: hir::TypeId) -> Option<KnownClass> {
        self.classes.get(&class_id).copied()
    }

    /// The resolved class identity of a known class, for consulting the visible
    /// instance environment.
    pub(in crate::typecheck) fn class_id(&self, known: KnownClass) -> Option<hir::TypeId> {
        self.ids.get(&known).copied()
    }

    /// A known class's method symbol by method name.
    pub(in crate::typecheck) fn method(&self, known: KnownClass, name: &str) -> Option<SymbolId> {
        self.methods.get(&(known, name.to_owned())).copied()
    }

    pub(in crate::typecheck) fn ordering(&self) -> Option<OrderingRep> {
        self.ordering
    }

    pub(in crate::typecheck) fn generic_rep(&self) -> Option<GenericRep> {
        self.generic
    }

    /// A program value by declaring module and name.
    pub(in crate::typecheck) fn value(&self, module: &str, name: &str) -> Option<SymbolId> {
        self.values
            .get(&(module.to_owned(), name.to_owned()))
            .copied()
    }

    /// `Data.Semigroup.append`.
    pub(in crate::typecheck) fn append(&self) -> Option<SymbolId> {
        self.value("Data.Semigroup", "append")
    }

    /// `Data.Monoid.mempty`.
    pub(in crate::typecheck) fn mempty(&self) -> Option<SymbolId> {
        self.value("Data.Monoid", "mempty")
    }

    /// The canonical `Control.Category.identity`, re-exported by Data.Function.
    /// Standalone source fixtures may provide the ordinary function directly.
    pub(in crate::typecheck) fn identity(&self) -> Option<SymbolId> {
        self.value("Control.Category", "identity")
            .or_else(|| self.value("Data.Function", "identity"))
    }

    /// `Control.Apply.apply`.
    pub(in crate::typecheck) fn apply(&self) -> Option<SymbolId> {
        self.value("Control.Apply", "apply")
    }

    /// `Control.Applicative.pure`.
    pub(in crate::typecheck) fn pure(&self) -> Option<SymbolId> {
        self.value("Control.Applicative", "pure")
    }
}

/// The declaration of `module.name` among the program's resolved types.
fn find_std_type<'a>(
    env: &'a SemanticEnv,
    module: &str,
    name: &str,
) -> Option<(hir::TypeId, &'a hir::TypeDeclaration)> {
    env.type_declarations.iter().find_map(|(id, declaration)| {
        (declaration.name == name && env.type_modules.get(id).map(String::as_str) == Some(module))
            .then_some((*id, declaration))
    })
}

fn nullary_constructor(declaration: &hir::TypeDeclaration, name: &str) -> Option<SymbolId> {
    declaration
        .constructors
        .iter()
        .find(|constructor| constructor.name == name && constructor.fields.is_empty())
        .map(|constructor| constructor.symbol)
}

fn constructor_symbol(declaration: &hir::TypeDeclaration, name: &str) -> Option<SymbolId> {
    declaration
        .constructors
        .iter()
        .find(|constructor| constructor.name == name)
        .map(|constructor| constructor.symbol)
}
