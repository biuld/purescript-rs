//! The record owner's immutable physical storage schema.
//!
//! Schema verification checks representation references and element shapes.
//! Equal lengths and unique sorted runtime labels are constructor obligations,
//! not facts that a representation-table check can prove about arbitrary arrays.

use super::{RefShape, Reference, ReprId, Representation, RepresentationTable, ValueShape};

/// Structural reads from the complete labelled store. Result payload recovery
/// remains an explicit checked conversion at the use site.
///
/// [`RecordOperation::Update`] replaces existing labelled slots in one fresh
/// store: every replacement label must exist in the runtime store (established
/// by construction and checked at runtime), the complete label/value sequence
/// otherwise stays the owner's single authoritative sequence, and the input
/// store is never mutated.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RecordOperation {
    Length,
    LabelAt {
        index: super::ValueId,
    },
    Find {
        label: super::ValueId,
    },
    Read {
        index: super::ValueId,
    },
    Update {
        replacements: Vec<(super::ValueId, super::ValueId)>,
    },
}

impl RecordOperation {
    /// Ordered operands with their physical shapes. Update pairs flatten to
    /// the alternating label/value order used by every consumer.
    pub(crate) fn arguments(&self) -> Vec<(super::ValueId, ValueShape)> {
        match self {
            Self::Length => Vec::new(),
            Self::LabelAt { index } | Self::Read { index } => {
                vec![(*index, ValueShape::Integer)]
            }
            Self::Find { label } => vec![(*label, ValueShape::String)],
            Self::Update { replacements } => replacements
                .iter()
                .flat_map(|(label, value)| {
                    [(*label, ValueShape::String), (*value, erased_argument())]
                })
                .collect(),
        }
    }

    pub(crate) fn result_shape(&self, storage: RecordStorage) -> ValueShape {
        match self {
            Self::Length | Self::Find { .. } => ValueShape::Integer,
            Self::LabelAt { .. } => ValueShape::String,
            Self::Read { .. } => super::payload::erased_shape(),
            Self::Update { .. } => ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(storage.representation()),
            }),
        }
    }

    /// Rebind the ordered operands positionally. Helper interning uses this to
    /// abstract concrete call-site ids into one shared helper per arity.
    pub(crate) fn with_arguments(self, arguments: Vec<super::ValueId>) -> Self {
        let mut remaining = arguments.into_iter();
        let mut take = |message| remaining.next().expect(message);
        match self {
            Self::Length => Self::Length,
            Self::LabelAt { .. } => Self::LabelAt {
                index: take("one argument"),
            },
            Self::Read { .. } => Self::Read {
                index: take("one argument"),
            },
            Self::Find { .. } => Self::Find {
                label: take("one label"),
            },
            Self::Update { replacements } => Self::Update {
                replacements: replacements
                    .into_iter()
                    .map(|_| (take("one label"), take("one value")))
                    .collect(),
            },
        }
    }
}

fn erased_argument() -> ValueShape {
    super::payload::erased_shape()
}

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(in crate::cc) struct RecordPlanning {
    storage: Option<RecordStorage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordStorage {
    representation: ReprId,
    labels: ReprId,
    values: ReprId,
}

impl RecordStorage {
    pub fn representation(self) -> ReprId {
        self.representation
    }

    pub fn labels(self) -> ReprId {
        self.labels
    }

    pub fn values(self) -> ReprId {
        self.values
    }

    fn verify(self, table: &RepresentationTable) -> Result<(), &'static str> {
        if table.representation(self.labels())
            != Some(&Representation::Array {
                element: ValueShape::String,
            })
            || table.representation(self.values())
                != Some(&Representation::Array {
                    element: super::payload::erased_shape(),
                })
        {
            return Err("record storage arrays have invalid label or erased-value elements");
        }
        if table.representation(self.representation())
            != Some(&Representation::Product {
                fields: vec![reference(self.labels()), reference(self.values())],
            })
            || table.product_labels(self.representation()).is_some()
        {
            return Err("record storage header does not match its registered array schema");
        }
        Ok(())
    }
}

impl RepresentationTable {
    pub(crate) fn ensure_record_storage(&mut self) -> RecordStorage {
        if let Some(storage) = self.record.storage {
            return storage;
        }
        let labels = super::layout::protocols::array(self, ValueShape::String);
        let values = super::layout::protocols::array(self, super::payload::erased_shape());
        let representation = self.reserve();
        self.set(
            representation,
            Representation::Product {
                fields: vec![reference(labels), reference(values)],
            },
        );
        let storage = RecordStorage {
            representation,
            labels,
            values,
        };
        self.record.storage = Some(storage);
        storage
    }

    pub(crate) fn record_storage(&self) -> Option<RecordStorage> {
        self.record.storage
    }

    pub(crate) fn verify_record_storage(&self) -> Result<(), &'static str> {
        if let Some(storage) = self.record_storage() {
            storage.verify(self)?;
        }
        Ok(())
    }
}

fn reference(id: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    })
}
