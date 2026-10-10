use super::*;

pub(super) fn verify(
    assignment: &Assignment,
    function: ValueId,
    signature: crate::cc::SignatureId,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    let plan = crate::cc::state::execution_projection(signature, table)
        .map_err(|message| assignment_error(assignment, message))?;
    require_value_shape(declared, function, closure_shape(signature), assignment)?;
    require_destination(
        declared,
        assignment,
        plan.payload,
        "state execution has an incompatible payload result",
    )
}
