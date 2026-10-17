//! The type facts analysis needs, supplied by each participating stage.
//!
//! One `Types` instance backs one whole analysis. The type checker supplies the
//! checked environment's constructor rows and record fields; the backend
//! supplies the core module's equivalents. Each method carries its own caller's
//! resolution step — walking an application spine, decoding alias shapes — so
//! the engine never reads one stage's internal representation. An incomplete,
//! unknown, or uninhabited shape answers `None` and analysis then uses the
//! default matrix path, matching the engine's documented policy.

use psrs_hir::SymbolId;

/// One data constructor's shape, in the coverage algebra.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorRow<Ty> {
    pub symbol: SymbolId,
    pub fields: Vec<Ty>,
}

/// The type facts one analysis consumes.
pub trait Types<Ty: Copy> {
    /// The analyzed type identity after removing a quantified prefix, where the
    /// supplying stage owns such a pass. Given the same identity, the answer is
    /// stable, so the engine may strip once and query repeatedly.
    fn shape(&self, ty: Ty) -> Ty;

    /// The element type of an `Array` application, if this is one.
    fn array_element(&self, ty: Ty) -> Option<Ty>;

    /// The complete constructor rows of a user data type. `None` means the
    /// resolved head is not a user data constructor group known complete. An
    /// uninhabited user type also answers `None`, so a wildcard matrix
    /// handles it without inventing rows for an empty set.
    fn constructors_of(&self, ty: Ty) -> Option<Vec<ConstructorRow<Ty>>>;

    /// Fields guaranteed by a checked record type, if this is one. Every
    /// analyzed pattern field must belong to this set. An open tail may add
    /// fields that record patterns do not inspect; suppliers need not invent
    /// those unknown fields. Closed types supply their complete field set.
    fn record_fields(&self, ty: Ty) -> Option<Vec<(String, Ty)>>;

    /// Whether this type is the `Boolean` constructor.
    fn boolean_type(&self, ty: Ty) -> bool;

    /// The name the witness renderer prints for a data constructor.
    fn constructor_name(&self, symbol: SymbolId) -> String;
}
