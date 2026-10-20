//! Scope and exit checking for generic native protection.
use super::*;
use crate::cc::{Region, RegionExit};

#[allow(clippy::too_many_arguments)]
pub(super) fn verify(
    assignment: &Assignment,
    body: &Region,
    exception: ValueId,
    handler: &Region,
    available: &HashSet<ValueId>,
    declared: &HashMap<ValueId, ValueShape>,
    signatures: &HashMap<SymbolId, Signature>,
    table: &RepresentationTable,
    functions: Option<&HashMap<SymbolId, &Function>>,
    span: TextRange,
    name: &str,
) -> Result<(), Vec<BackendError>> {
    let token = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Exception,
    });
    require_value_shape(declared, exception, token, assignment)?;
    if available.contains(&exception) || exception == assignment.destination {
        return Err(assignment_error(
            assignment,
            "exception binder is not fresh",
        ));
    }
    for (region, catching) in [(body, false), (handler, true)] {
        let mut scope = available.clone();
        if catching {
            scope.insert(exception);
        }
        verify_assignments(
            &region.assignments,
            &mut scope,
            declared,
            signatures,
            table,
            functions,
            span,
            name,
        )?;
        let (RegionExit::Yield(value) | RegionExit::Propagate(value)) = region.exit;
        if !scope.contains(&value) {
            return Err(undef_error(assignment.span, span));
        }
        let expected = match region.exit {
            RegionExit::Yield(_) => declared_shape(declared, assignment.destination, assignment)?,
            RegionExit::Propagate(_) => token,
        };
        require_value_shape(declared, value, expected, assignment)?;
    }
    Ok(())
}
