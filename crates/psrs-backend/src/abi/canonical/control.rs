//! Control callbacks cannot store call-scoped canonical values.
use super::{CanonicalType, Ownership};

impl CanonicalType {
    /// A borrowed handle, or any aggregate that contains one.
    ///
    /// Call-scoped `borrow<T>` cannot be stored in a software continuation: the
    /// host releases it when the canonical call returns. Owned handles and
    /// GC-copied strings/lists are not this predicate; they have a different
    /// lifetime owner.
    pub(crate) fn contains_borrowed_handle(&self) -> bool {
        match self {
            Self::Handle {
                ownership: Ownership::Borrow,
                ..
            } => true,
            Self::List(inner) | Self::Option(inner) => inner.contains_borrowed_handle(),
            Self::FixedList { element, .. } => element.contains_borrowed_handle(),
            Self::Record(fields) => fields
                .iter()
                .any(|field| field.ty.contains_borrowed_handle()),
            Self::Variant(cases) => cases.iter().any(|case| {
                case.payload
                    .as_deref()
                    .is_some_and(Self::contains_borrowed_handle)
            }),
            Self::Result { ok, err } => {
                ok.as_deref().is_some_and(Self::contains_borrowed_handle)
                    || err.as_deref().is_some_and(Self::contains_borrowed_handle)
            }
            _ => false,
        }
    }
}

/// Control callbacks may not close over a borrowed canonical handle.
pub(crate) fn reject_borrowed_control_capture<'a>(
    types: impl IntoIterator<Item = &'a CanonicalType>,
) -> Result<(), &'static str> {
    if types
        .into_iter()
        .any(CanonicalType::contains_borrowed_handle)
    {
        Err("control callback cannot capture a borrowed host resource handle")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::ResourceId;
    use super::*;
    use psrs_hir::{ModuleId, SymbolId};

    fn handle(borrowed: bool) -> CanonicalType {
        CanonicalType::Handle {
            resource: ResourceId {
                interface: "fixture:io/files@0.1.0".into(),
                name: "file".into(),
            },
            ownership: if borrowed {
                Ownership::Borrow
            } else {
                Ownership::Own {
                    drop: SymbolId::new(ModuleId(0), 1),
                }
            },
        }
    }

    #[test]
    fn borrowed_handles_are_unsupported_control_captures() {
        let borrowed = handle(true);
        let owned = handle(false);
        assert!(borrowed.contains_borrowed_handle());
        assert!(CanonicalType::Option(Box::new(borrowed.clone())).contains_borrowed_handle());
        assert!(CanonicalType::List(Box::new(borrowed.clone())).contains_borrowed_handle());
        assert!(
            CanonicalType::Record(vec![super::super::CanonicalField {
                name: "file".into(),
                ty: borrowed.clone(),
            }])
            .contains_borrowed_handle()
        );
        assert!(reject_borrowed_control_capture([&borrowed]).is_err());
        assert!(!owned.contains_borrowed_handle());
        assert!(!CanonicalType::String.contains_borrowed_handle());
        assert!(reject_borrowed_control_capture([&owned, &CanonicalType::String]).is_ok());
    }
}
