use super::*;
use crate::cc::{AggregateConvert, ValueConversion};

impl StateCallProjection {
    /// Projects a checked Step map to its payload conversion. The surrounding
    /// dependency checker must separately prove that the State field is current.
    pub fn payload_conversion(
        conversion: &AggregateConvert,
        table: &RepresentationTable,
    ) -> Result<AggregateConvert, &'static str> {
        let read = |result| {
            Self::checked(
                &Signature {
                    parameters: vec![ValueShape::State],
                    result,
                },
                table,
            )?
            .ok_or("Step conversion has no logical dependency field")
        };
        let source = read(conversion.source)?;
        let target = read(conversion.destination)?;
        let plan = match &conversion.plan {
            ValueConversion::Identity if conversion.source == conversion.destination => {
                ValueConversion::Identity
            }
            ValueConversion::ProductMap {
                source: from,
                target: to,
                fields,
                ..
            } if conversion.source == product(*from)
                && conversion.destination == product(*to)
                && source.state_field == target.state_field
                && fields.len() == 2
                && fields[source.state_field] == ValueConversion::Identity =>
            {
                fields[source.payload_field].clone()
            }
            _ => return Err("Step conversion requires checked payload transport"),
        };
        Ok(AggregateConvert {
            source: source.payload,
            destination: target.payload,
            plan,
        })
    }
}

fn product(id: crate::cc::ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    })
}
