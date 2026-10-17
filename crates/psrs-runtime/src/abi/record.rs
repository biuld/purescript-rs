//! Source contract for compiler-projected operations on the shared record store.

pub const RECORD_STORAGE_MODULE: &str = "psrs:record-storage";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecordPrimitive {
    Length,
    KeyAt,
    PayloadAt,
    Build,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordSourceValue {
    Record,
    Int,
    String,
    Slot,
    ArrayString,
    ArraySlot,
}

pub struct RecordSourceContract {
    pub parameters: &'static [RecordSourceValue],
    pub result: RecordSourceValue,
    pub projection: RecordProjection,
}

/// How source operands become an operation on the shared record owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordProjection {
    Length { record: usize },
    KeyAt { index: usize, record: usize },
    PayloadAt { index: usize, record: usize },
    Build { labels: usize, values: usize },
}

impl RecordPrimitive {
    pub const ALL: [Self; 4] = [Self::Length, Self::KeyAt, Self::PayloadAt, Self::Build];

    pub fn from_export(export: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|primitive| primitive.export() == export)
    }

    pub const fn export(self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::KeyAt => "key_at",
            Self::PayloadAt => "payload_at",
            Self::Build => "build",
        }
    }

    pub const fn source_contract(self) -> RecordSourceContract {
        use RecordSourceValue::*;
        match self {
            Self::Length => RecordSourceContract {
                parameters: &[Record],
                result: Int,
                projection: RecordProjection::Length { record: 0 },
            },
            Self::KeyAt => RecordSourceContract {
                parameters: &[Int, Record],
                result: String,
                projection: RecordProjection::KeyAt {
                    index: 0,
                    record: 1,
                },
            },
            Self::PayloadAt => RecordSourceContract {
                parameters: &[Int, Record],
                result: Slot,
                projection: RecordProjection::PayloadAt {
                    index: 0,
                    record: 1,
                },
            },
            Self::Build => RecordSourceContract {
                parameters: &[ArrayString, ArraySlot],
                result: Record,
                projection: RecordProjection::Build {
                    labels: 0,
                    values: 1,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_are_exact_and_complete() {
        for primitive in RecordPrimitive::ALL {
            assert_eq!(
                RecordPrimitive::from_export(primitive.export()),
                Some(primitive)
            );
        }
        assert_eq!(RecordPrimitive::from_export("update"), None);
    }
}
