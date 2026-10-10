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
    /// A logical sequencing dependency with no physical payload. Checked Core
    /// owns nominal region agreement; CC must retain producer/use provenance.
    State,
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

/// One projected field of a guest aggregate: the concrete source value at that
/// field and the storage slot it maps to in the physical representation.
///
/// `value` is the instance-aware guest node read or written by MIR; `stored` is
/// the physical field shape from the [`RepresentationTable`]. They differ for a
/// parameterized ADT's type-parameter field, where the table stores `Erased`
/// (DEC-07) but the resolved Core type names a concrete payload. `value ==
/// stored` is a direct projection; otherwise a defined CC conversion (scalar
/// `<->` erased, concrete reference `<->` erased) bridges them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub value: GuestLayout,
    pub stored: ValueShape,
}

impl Field {
    /// The shape of the concrete value node.
    pub fn value_shape(&self) -> ValueShape {
        self.value.shape()
    }
}

/// One case of a projected guest variant, in tag order. `fields` are the
/// projected fields of the case's single payload (empty for a nullary case).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestCase {
    pub tag: u32,
    pub fields: Vec<Field>,
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
    /// projection of each field in storage order.
    Product {
        repr: ReprId,
        labels: Vec<String>,
        fields: Vec<Field>,
    },
    /// A tagged value with its representation and cases in tag order.
    Variant { repr: ReprId, cases: Vec<GuestCase> },
    /// A GC array with its representation and projected element.
    Array { repr: ReprId, element: Box<Field> },
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

/// Resolves a value shape to its storage guest layout by looking up the
/// representation table. Each field pairs the storage shape with a projection
/// that resolves the same storage shape, so `value == stored`. A representation
/// handle yields its product, variant, array, or boxed node; every other shape
/// is a scalar.
pub(crate) fn guest_layout(shape: ValueShape, table: &RepresentationTable) -> Option<GuestLayout> {
    let mut visited = std::collections::HashSet::new();
    guest_layout_inner(shape, table, &mut visited)
}

fn guest_layout_inner(
    shape: ValueShape,
    table: &RepresentationTable,
    visited: &mut std::collections::HashSet<ReprId>,
) -> Option<GuestLayout> {
    match shape {
        ValueShape::Reference(Reference {
            heap: RefShape::Repr(repr),
            ..
        }) => {
            if !visited.insert(repr) {
                // A recursive representation stops here; MIR resolves nested
                // shapes lazily through the table.
                return Some(GuestLayout::Scalar { shape });
            }
            let layout = Some(match table.representation(repr)? {
                Representation::Box { value } => GuestLayout::Boxed { shape: *value },
                Representation::Product { fields } => GuestLayout::Product {
                    repr,
                    labels: table
                        .product_labels(repr)
                        .map(<[String]>::to_vec)
                        .unwrap_or_default(),
                    fields: fields
                        .iter()
                        .map(|field| storage_field(*field, table, visited))
                        .collect::<Option<Vec<_>>>()?,
                },
                Representation::Variant { cases } => GuestLayout::Variant {
                    repr,
                    cases: cases
                        .iter()
                        .map(|case| storage_case(case, table, visited))
                        .collect::<Option<Vec<_>>>()?,
                },
                Representation::Array { element } => GuestLayout::Array {
                    repr,
                    element: Box::new(storage_field(*element, table, visited)?),
                },
            });
            visited.remove(&repr);
            layout
        }
        other => Some(GuestLayout::Scalar { shape: other }),
    }
}

/// A storage field whose concrete node is the resolution of its own storage
/// shape.
fn storage_field(
    shape: ValueShape,
    table: &RepresentationTable,
    visited: &mut std::collections::HashSet<ReprId>,
) -> Option<Field> {
    Some(Field {
        value: guest_layout_inner(shape, table, visited)?,
        stored: shape,
    })
}

fn storage_case(
    case: &VariantCase,
    table: &RepresentationTable,
    visited: &mut std::collections::HashSet<ReprId>,
) -> Option<GuestCase> {
    Some(GuestCase {
        tag: case.tag,
        fields: case
            .fields
            .iter()
            .map(|shape| storage_field(*shape, table, visited))
            .collect::<Option<Vec<_>>>()?,
    })
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
