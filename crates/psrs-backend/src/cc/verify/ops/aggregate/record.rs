use super::*;

pub(super) fn verify_record_plan(
    assignment: &Assignment,
    plan: &ValueConversion,
    source_shape: ValueShape,
    table: &RepresentationTable,
) -> Result<ValueShape, Vec<BackendError>> {
    let (source, target, labels, fields, packing) = match plan {
        ValueConversion::RecordPack {
            source,
            target,
            labels,
            fields,
        } => (*source, *target, labels, fields, true),
        ValueConversion::RecordUnpack {
            source,
            target,
            labels,
            fields,
        } => (*source, *target, labels, fields, false),
        _ => unreachable!("record verifier only accepts record conversion plans"),
    };
    let fail = |message| assignment_error(assignment, message);
    table.verify_record_storage().map_err(fail)?;
    let storage = table
        .record_storage()
        .ok_or_else(|| fail("record conversion has no registered storage schema"))?;
    let product = if packing { source } else { target };
    let store = if packing { target } else { source };
    let Some(Representation::Product {
        fields: product_fields,
    }) = table.representation(product)
    else {
        return Err(fail("record conversion has no closed product layout"));
    };
    if store != storage.representation()
        || source_shape != repr_shape(source)
        || table.product_labels(product) != Some(labels.as_slice())
        || fields.len() != product_fields.len()
        || fields.len() != labels.len()
        || labels.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(fail(
            "record conversion endpoints, labels or arity are invalid",
        ));
    }
    for (plan, product_field) in fields.iter().zip(product_fields) {
        let (input, output) = if packing {
            (*product_field, erased_shape())
        } else {
            (erased_shape(), *product_field)
        };
        if super::verify_plan(assignment, plan, input, table)? != output {
            return Err(fail(
                "record field conversion does not match its stored payload policy",
            ));
        }
    }
    Ok(repr_shape(target))
}
