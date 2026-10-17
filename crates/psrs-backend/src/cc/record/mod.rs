//! The record owner's immutable physical storage schema.
//!
//! Schema verification checks representation references and element shapes.
//! Equal lengths and unique sorted runtime labels are constructor obligations,
//! not facts that a representation-table check can prove about arbitrary arrays.

use super::{RefShape, Reference, ReprId, Representation, RepresentationTable, ValueShape};

/// Structural reads from the complete labelled store. Result payload recovery
/// remains an explicit checked conversion at the use site.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RecordOperation {
    Length,
    LabelAt { index: super::ValueId },
    Find { label: super::ValueId },
    Read { index: super::ValueId },
}

impl RecordOperation {
    pub(crate) fn argument(&self) -> Option<(super::ValueId, ValueShape)> {
        match self {
            Self::Length => None,
            Self::LabelAt { index } | Self::Read { index } => Some((*index, ValueShape::Integer)),
            Self::Find { label } => Some((*label, ValueShape::String)),
        }
    }

    pub(crate) fn result_shape(&self) -> ValueShape {
        match self {
            Self::Length | Self::Find { .. } => ValueShape::Integer,
            Self::LabelAt { .. } => ValueShape::String,
            Self::Read { .. } => super::payload::erased_shape(),
        }
    }

    pub(crate) fn with_argument(&self, argument: super::ValueId) -> Self {
        match self {
            Self::Length => Self::Length,
            Self::LabelAt { .. } => Self::LabelAt { index: argument },
            Self::Find { .. } => Self::Find { label: argument },
            Self::Read { .. } => Self::Read { index: argument },
        }
    }
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
