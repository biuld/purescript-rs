//! Target-neutral representation requirements used by CC.
//!
//! CC deliberately does not use the Wasm value/type model.  These handles are
//! resolved by a target planner while lowering CC to MIR.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReprId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignatureId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefShape {
    /// A value whose physical type is selected by the target planner.
    Repr(ReprId),
    /// A reference to an aggregate value without committing to a concrete
    /// constructor or object representation.
    Aggregate,
    /// The erased representation used by polymorphic values.
    Erased,
    /// A closure value with the given abstract call signature.
    Closure(SignatureId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reference {
    pub nullable: bool,
    pub heap: RefShape,
}

/// A logical CC value shape. The scalar spellings describe source-level
/// runtime requirements; P9 decides whether they become Wasm scalars,
/// handles, or another target representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueShape {
    Integer,
    Boolean,
    Number,
    Reference(Reference),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signature {
    pub parameters: Vec<ValueShape>,
    pub result: ValueShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDecl {
    pub id: super::ValueId,
    pub ty: ValueShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantCase {
    pub tag: u32,
    pub fields: Vec<ValueShape>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Representation {
    Box { value: ValueShape },
    Product { fields: Vec<ValueShape> },
    Variant { cases: Vec<VariantCase> },
    Array { element: ValueShape },
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RepresentationTable {
    pub representations: Vec<Representation>,
    pub signatures: Vec<Signature>,
    /// Canonical logical labels for record products; positional products do
    /// not have an entry here.
    pub product_labels: std::collections::HashMap<ReprId, Vec<String>>,
}

impl RepresentationTable {
    pub fn reserve(&mut self) -> ReprId {
        let id = ReprId(self.representations.len() as u32);
        // A temporary product is replaced before the table leaves P8. Keeping
        // reservation explicit lets recursive references use stable handles.
        self.representations
            .push(Representation::Product { fields: Vec::new() });
        id
    }

    pub fn set(&mut self, id: ReprId, representation: Representation) {
        self.representations[id.0 as usize] = representation;
    }

    pub fn set_product_labels(&mut self, id: ReprId, labels: Vec<String>) {
        self.product_labels.insert(id, labels);
    }

    pub fn product_labels(&self, id: ReprId) -> Option<&[String]> {
        self.product_labels.get(&id).map(Vec::as_slice)
    }

    pub fn add_signature(&mut self, signature: Signature) -> SignatureId {
        let id = SignatureId(self.signatures.len() as u32);
        self.signatures.push(signature);
        id
    }

    pub fn representation(&self, id: ReprId) -> Option<&Representation> {
        self.representations.get(id.0 as usize)
    }

    pub fn signature(&self, id: SignatureId) -> Option<&Signature> {
        self.signatures.get(id.0 as usize)
    }
}
