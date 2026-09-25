use psrs_hir::BuiltinType;

pub(in crate::resolver) fn split_qualified(text: &str) -> Option<(&str, &str)> {
    if let Some(index) = text.rfind(".(")
        && text.ends_with(')')
    {
        return Some((&text[..index], &text[index + 2..text.len() - 1]));
    }
    let index = text.rfind('.')?;
    Some((&text[..index], &text[index + 1..]))
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
