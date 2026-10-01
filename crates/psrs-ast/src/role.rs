use crate::Name;
use psrs_span::TextRange;

/// A source role annotation immediately following its data, newtype, or
/// foreign-data declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleDeclaration {
    pub name: Name,
    pub roles: Vec<RoleAnnotation>,
    pub span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeRole {
    Nominal,
    Representational,
    Phantom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleAnnotation {
    pub role: TypeRole,
    pub span: TextRange,
}
