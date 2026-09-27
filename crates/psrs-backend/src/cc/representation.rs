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
    /// A string is physically an `i32` ABI pointer on the current target, but
    /// it keeps a distinct semantic shape so the verifier rejects numeric
    /// operations on it and the erased protocol boxes it explicitly.
    String,
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

/// The concrete guest layout of one bound value, resolved from CC's
/// representation table. This is the guest half of a WIT binding: the canonical
/// type says how bytes cross the boundary; this says how the guest stores the
/// value. It is a view over the same [`RepresentationTable`], not a second
/// description of the declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuestLayout {
    /// An `Integer`, `Boolean`, `Number`, or `String` value.
    Scalar { shape: ValueShape },
    /// A value boxed into an erased field.
    Boxed { shape: ValueShape },
    /// A closed record with its representation, canonical labels, and the
    /// shape of each field in storage order.
    Product {
        repr: ReprId,
        labels: Vec<String>,
        fields: Vec<ValueShape>,
    },
    /// A tagged value with its representation and cases in tag order.
    Variant {
        repr: ReprId,
        cases: Vec<VariantCase>,
    },
    /// A GC array with its representation and element shape.
    Array { repr: ReprId, element: ValueShape },
}

impl GuestLayout {
    /// The abstract shape of this layout, for the erased aggregate protocol.
    pub fn shape(&self) -> ValueShape {
        match self {
            GuestLayout::Scalar { shape } | GuestLayout::Boxed { shape } => *shape,
            GuestLayout::Product { repr, .. }
            | GuestLayout::Variant { repr, .. }
            | GuestLayout::Array { repr, .. } => ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(*repr),
            }),
        }
    }
}

/// Resolves a value shape to its recursive guest layout by looking up the
/// representation table. A representation handle yields its product, variant,
/// array, or boxed node; every other shape is a scalar. This is the one
/// recursive accessor the ABI lowering walks in lockstep with a canonical type.
pub(crate) fn guest_layout(shape: ValueShape, table: &RepresentationTable) -> Option<GuestLayout> {
    match shape {
        ValueShape::Reference(Reference {
            heap: RefShape::Repr(repr),
            ..
        }) => Some(match table.representation(repr)? {
            Representation::Box { value } => GuestLayout::Boxed { shape: *value },
            Representation::Product { fields } => GuestLayout::Product {
                repr,
                labels: table
                    .product_labels(repr)
                    .map(<[String]>::to_vec)
                    .unwrap_or_default(),
                fields: fields.clone(),
            },
            Representation::Variant { cases } => GuestLayout::Variant {
                repr,
                cases: cases.clone(),
            },
            Representation::Array { element } => GuestLayout::Array {
                repr,
                element: *element,
            },
        }),
        other => Some(GuestLayout::Scalar { shape: other }),
    }
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
