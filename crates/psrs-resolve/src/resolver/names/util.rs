use psrs_hir::BuiltinType;

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

pub(in crate::resolver) fn prim_type(name: &str) -> Option<BuiltinType> {
    PRIM_TYPES
        .iter()
        .find_map(|(prim_name, builtin)| (*prim_name == name).then_some(*builtin))
}
