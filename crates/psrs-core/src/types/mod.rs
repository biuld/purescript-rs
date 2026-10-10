//! The Core type spine.
//!
//! A type is a variable, a constructor, an application, a quantifier, a row, or
//! a type-level literal. An arrow is the `Function` application spine, a
//! record is `Record` applied to a row, and an effect stays an ordinary
//! application of a user constructor; the accessors below read those shapes
//! from the module type table rather than leaving each pass to match on it.

use psrs_hir::{TypeId as HirTypeId, TypeVariableId};

mod call;
pub use call::call_parts;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// A type constructor reference, mirrored from THIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeConstructor {
    Function,
    Record,
    /// The `Prim.Row` type constructor, declared with a phantom role. Its
    /// application is a nominal type, not a row value.
    Row,
    Array,
    Int,
    Number,
    Boolean,
    String,
    Char,
    Unit,
    /// The `Prim.Type` kind constructor, of kind `Type`.
    Type,
    /// The `Prim.Constraint` kind constructor, of kind `Type`.
    Constraint,
    /// The `Prim.Symbol` kind constructor, of kind `Type`.
    Symbol,
    User(HirTypeId),
}

impl TypeConstructor {
    /// Whether this constructor names one of the source primitive scalars.
    pub fn is_primitive(self) -> bool {
        matches!(
            self,
            Self::Int | Self::Number | Self::Boolean | Self::String | Self::Char | Self::Unit
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// A generalized type variable; quantifiers are stored at each binding site.
    Variable(TypeVariableId),
    Constructor(TypeConstructor),
    Application(TypeId, TypeId),
    /// A lexical type-level quantifier. Nested nodes retain independent scope.
    ForAll {
        variables: Vec<TypeVariableId>,
        body: TypeId,
    },
    /// A closure created with a fixed parameter list. The result is a value,
    /// even when that value is a function or another closure. Representation
    /// lowering emits this for an effect; source arrows stay [`TypeConstructor::Function`].
    Closure {
        parameters: Vec<TypeId>,
        result: TypeId,
    },
    /// The empty row. A closed record's row ends here.
    RowEmpty,
    /// A row extended with one labeled field. A record type is
    /// `Application(Constructor(Record), row)`; a closed row ends in
    /// [`Type::RowEmpty`] and an open row ends in a [`Type::Variable`].
    RowExtend {
        label: String,
        ty: TypeId,
        tail: TypeId,
    },
    /// A type-level string literal, of kind `Symbol`. The payload is a sequence
    /// of Unicode scalar values (DEC-16), so it never holds an unpaired
    /// surrogate. Two literals are equal when their sequences are equal.
    TypeLevelString(String),
    /// A type-level integer literal, of kind `Int`. Two are equal when their
    /// values are equal.
    TypeLevelInt(i64),
}

/// The parameter and result of an arrow type `a -> b`, spelled as the
/// application spine `Application(Application(Constructor(Function), a), b)`.
pub fn arrow_parts(types: &[Type], id: TypeId) -> Option<(TypeId, TypeId)> {
    let Type::Application(inner, result) = types.get(id.0 as usize)? else {
        return None;
    };
    let Type::Application(head, parameter) = types.get(inner.0 as usize)? else {
        return None;
    };
    matches!(
        types.get(head.0 as usize),
        Some(Type::Constructor(TypeConstructor::Function))
    )
    .then_some((*parameter, *result))
}

/// The parameter list and result of a fixed-arity closure type.
pub fn closure_parts(types: &[Type], id: TypeId) -> Option<(&[TypeId], TypeId)> {
    match types.get(id.0 as usize)? {
        Type::Closure { parameters, result } => Some((parameters, *result)),
        _ => None,
    }
}

/// The binders and body of a type-level universal quantifier.
pub fn forall_parts(types: &[Type], id: TypeId) -> Option<(&[TypeVariableId], TypeId)> {
    match types.get(id.0 as usize)? {
        Type::ForAll { variables, body } => Some((variables, *body)),
        _ => None,
    }
}

/// Reads a binding's leading lexical quantifiers without crossing an arrow.
/// Returns `None` for a dangling or cyclic spine; callers must report invalid IR.
/// The variable identities and their order are preserved for declaration scope.
pub fn scheme_parts(types: &[Type], mut id: TypeId) -> Option<(Vec<TypeVariableId>, TypeId)> {
    let mut quantified = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while seen.insert(id) {
        match types.get(id.0 as usize)? {
            Type::ForAll { variables, body } => {
                quantified.extend_from_slice(variables);
                id = *body;
            }
            _ => return Some((quantified, id)),
        }
    }
    None
}

#[cfg(test)]
mod scheme_tests {
    use super::*;

    #[test]
    fn binding_scheme_preserves_variables_and_stops_before_a_result_quantifier() {
        let a = TypeVariableId(7);
        let b = TypeVariableId(8);
        let types = vec![
            Type::Constructor(TypeConstructor::Function),
            Type::Variable(a),
            Type::Variable(b),
            Type::ForAll {
                variables: vec![b],
                body: TypeId(2),
            },
            Type::Application(TypeId(0), TypeId(1)),
            Type::Application(TypeId(4), TypeId(3)),
            Type::ForAll {
                variables: vec![a],
                body: TypeId(5),
            },
        ];
        assert_eq!(scheme_parts(&types, TypeId(6)), Some((vec![a], TypeId(5))));
    }

    #[test]
    fn invalid_scheme_spines_are_rejected_without_fresh_unknowns() {
        let types = vec![Type::ForAll {
            variables: vec![TypeVariableId(0)],
            body: TypeId(0),
        }];
        assert!(scheme_parts(&types, TypeId(0)).is_none());
        assert!(scheme_parts(&types, TypeId(1)).is_none());
    }
}
