use psrs_hir::{BuiltinType, TypeReference};

pub(in crate::resolver) const PRIM_TYPES: [(&str, BuiltinType); 12] = [
    ("Array", BuiltinType::Array),
    ("Boolean", BuiltinType::Boolean),
    ("Char", BuiltinType::Char),
    ("Constraint", BuiltinType::Constraint),
    ("Function", BuiltinType::Function),
    ("Int", BuiltinType::Int),
    ("Number", BuiltinType::Number),
    ("Record", BuiltinType::Record),
    ("Row", BuiltinType::Row),
    ("String", BuiltinType::String),
    ("Symbol", BuiltinType::Symbol),
    ("Type", BuiltinType::Type),
];

pub(in crate::resolver) fn split_qualified(text: &str) -> Option<(&str, &str)> {
    if let Some(index) = text.rfind(".(")
        && text.ends_with(')')
    {
        let qualifier = &text[..index];
        return is_module_qualifier(qualifier)
            .then_some((qualifier, &text[index + 2..text.len() - 1]));
    }
    let index = text.rfind('.')?;
    let qualifier = &text[..index];
    let member = &text[index + 1..];
    (is_module_qualifier(qualifier) && !member.is_empty()).then_some((qualifier, member))
}

fn is_module_qualifier(qualifier: &str) -> bool {
    !qualifier.is_empty()
        && qualifier
            .split('.')
            .all(|part| part.chars().next().is_some_and(char::is_uppercase))
}

pub(in crate::resolver) fn is_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_uppercase())
}

pub(in crate::resolver) fn builtin_type(name: &str) -> Option<BuiltinType> {
    Some(match name {
        "Int" => BuiltinType::Int,
        "Number" => BuiltinType::Number,
        "Boolean" => BuiltinType::Boolean,
        "String" => BuiltinType::String,
        "Char" => BuiltinType::Char,
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

/// `Partial` lives in `Prim` and is in scope in every module, the same way
/// `Int` is. Source does not import it.
pub(in crate::resolver) fn implicit_prim_class(name: &str) -> Option<TypeReference> {
    psrs_hir::primitive_type_declarations()
        .into_iter()
        .find(|(owner, declaration)| *owner == "Prim" && declaration.name == name)
        .map(|(_, declaration)| TypeReference::Named(declaration.id))
}

pub(in crate::resolver) fn prim_type(name: &str) -> Option<BuiltinType> {
    PRIM_TYPES
        .iter()
        .find_map(|(prim_name, builtin)| (*prim_name == name).then_some(*builtin))
}

#[cfg(test)]
mod tests {
    use super::split_qualified;

    #[test]
    fn symbolic_operator_names_are_not_split_as_qualified_names() {
        assert_eq!(split_qualified(".&."), None);
        assert_eq!(
            split_qualified("Data.Int.Bits.and"),
            Some(("Data.Int.Bits", "and"))
        );
        assert_eq!(
            split_qualified("Data.Int.Bits.(.&.)"),
            Some(("Data.Int.Bits", ".&."))
        );
    }
}
