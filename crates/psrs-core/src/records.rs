use crate::{Module, Type, TypeConstructor, TypeId};

/// A row flattened into its fields and its tail. The tail is `None` for a
/// closed row and `Some(variable)` for an open row.
pub type RowFields = (Vec<(String, TypeId)>, Option<TypeId>);

/// The row of a record type `Application(Constructor(Record), row)`, or `None`
/// when `id` is not a record type.
pub fn record_row(types: &[Type], id: TypeId) -> Option<TypeId> {
    let Type::Application(function, row) = types.get(id.0 as usize)? else {
        return None;
    };
    matches!(
        types.get(function.0 as usize),
        Some(Type::Constructor(TypeConstructor::Record))
    )
    .then_some(*row)
}

/// Flattens a row into its fields and its tail. The tail is `None` for a closed
/// row and `Some(variable)` for an open row. `None` is returned when `row`
/// reaches a node that is neither a row constructor nor a row variable.
pub fn row_fields(types: &[Type], mut row: TypeId) -> Option<RowFields> {
    let mut fields = Vec::new();
    loop {
        match types.get(row.0 as usize)? {
            Type::RowEmpty => return Some((fields, None)),
            Type::RowExtend { label, ty, tail } => {
                fields.push((label.clone(), *ty));
                row = *tail;
            }
            Type::Variable(_) => return Some((fields, Some(row))),
            _ => return None,
        }
    }
}

impl Module {
    /// The row of a record type `Application(Constructor(Record), row)`, or
    /// `None` when `id` is not a record type.
    pub fn record_row(&self, id: TypeId) -> Option<TypeId> {
        record_row(&self.types, id)
    }

    /// Flattens a row into its fields and its tail. The tail is `None` for a
    /// closed row and `Some(variable)` for an open row.
    pub fn row_fields(&self, row: TypeId) -> Option<RowFields> {
        row_fields(&self.types, row)
    }

    /// The fields of a record type in canonical (label-sorted) order, or `None`
    /// when `id` is not a record type.
    pub fn record_fields(&self, id: TypeId) -> Option<Vec<(String, TypeId)>> {
        let row = self.record_row(id)?;
        let (mut fields, _) = self.row_fields(row)?;
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        Some(fields)
    }

    /// The type of one field of a record type, or `None` when `id` is not a
    /// record type or the label is absent.
    pub fn record_field(&self, id: TypeId, label: &str) -> Option<TypeId> {
        self.record_fields(id)?
            .into_iter()
            .find(|(candidate, _)| candidate == label)
            .map(|(_, ty)| ty)
    }

    /// Whether `id` is a record type, open or closed.
    pub fn is_record_type(&self, id: TypeId) -> bool {
        self.record_row(id).is_some()
    }

    /// Whether a record type's row is open (ends in a variable). `None` when
    /// `id` is not a record type.
    pub fn record_is_open(&self, id: TypeId) -> Option<bool> {
        let row = self.record_row(id)?;
        let (_, tail) = self.row_fields(row)?;
        Some(tail.is_some())
    }
}
