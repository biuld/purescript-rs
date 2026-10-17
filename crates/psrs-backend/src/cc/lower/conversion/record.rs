use super::*;
use crate::cc::Representation;

impl FunctionLowerer<'_> {
    pub(super) fn record_storage_conversion(
        &mut self,
        source: ValueShape,
        target: ValueShape,
        span: TextRange,
    ) -> Result<Option<ValueConversion>, Vec<BackendError>> {
        let Some(storage) = self.representations.record_storage() else {
            return Ok(None);
        };
        let repr = |shape| match shape {
            ValueShape::Reference(Reference {
                heap: RefShape::Repr(id),
                nullable: false,
            }) => Some(id),
            _ => None,
        };
        let (Some(source), Some(target)) = (repr(source), repr(target)) else {
            return Ok(None);
        };
        let packing = target == storage.representation();
        let unpacking = source == storage.representation();
        if packing == unpacking {
            return Ok(None);
        }
        let product = if packing { source } else { target };
        let labels = self
            .representations
            .product_labels(product)
            .ok_or_else(|| {
                conversion_error(
                    span,
                    "record storage boundary has no complete closed labels",
                )
            })?
            .to_vec();
        let Some(Representation::Product { fields }) = self.representations.representation(product)
        else {
            return Err(conversion_error(
                span,
                "record storage boundary requires a closed product",
            ));
        };
        let fields = fields
            .clone()
            .into_iter()
            .map(|field| {
                if packing {
                    self.erase_payload(field, span)
                } else {
                    self.recover_payload(field, span)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(if packing {
            ValueConversion::RecordPack {
                source,
                target,
                labels,
                fields,
            }
        } else {
            ValueConversion::RecordUnpack {
                source,
                target,
                labels,
                fields,
            }
        }))
    }
}
