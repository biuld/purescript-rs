use super::{Expr, ExprKind, LowerError, TypeId};
use psrs_thir::{Evidence, EvidenceKind, Type};

/// Erases checked class evidence into the existing Core value and call forms.
pub(super) fn lower_evidence(evidence: &Evidence, types: &[Type]) -> Result<Expr, LowerError> {
    let span = evidence.span;
    let ty = TypeId(evidence.ty.0);
    let kind = match &evidence.kind {
        EvidenceKind::Given(id) => ExprKind::Local(*id),
        EvidenceKind::Global(symbol) => ExprKind::Global(*symbol),
        EvidenceKind::Coercible { .. } | EvidenceKind::Primitive { .. } => {
            // A `Proof` member leaves no runtime value and a `Relation` member's
            // dictionary is empty, so both erase to the same empty record at the
            // dictionary type the evidence already carries.
            ExprKind::Record { fields: Vec::new() }
        }
        EvidenceKind::Superclass { parent, field } => ExprKind::FieldAccess {
            record: Box::new(lower_evidence(parent, types)?),
            field: field.clone(),
        },
        EvidenceKind::Instance {
            constructor,
            constructor_type,
            context,
        } => {
            let mut function_type = *constructor_type;
            let mut function = Expr {
                kind: ExprKind::Global(*constructor),
                ty: TypeId(function_type.0),
                span,
            };
            for evidence_argument in context {
                let Some((_, result)) = psrs_thir::arrow_parts(types, function_type) else {
                    return Err(LowerError {
                        span: evidence_argument.span,
                        message: "instance dictionary constructor takes too few context arguments",
                    });
                };
                let argument = lower_evidence(evidence_argument, types)?;
                // lower_module_inner verifies THIR before lowering. Context
                // types are compared there by semantic equality, which also
                // accepts separately interned alpha-equivalent method foralls.
                function = Expr {
                    kind: ExprKind::Application(Box::new(function), Box::new(argument)),
                    ty: TypeId(result.0),
                    span,
                };
                function_type = result;
            }
            // THIR has already checked this application with semantic type
            // equality. Its result can be represented by a distinct TypeId
            // (for example, after solving a derived Generic representation),
            // so leave the applied constructor's result type intact; Core's
            // verifier compares it semantically at the evidence use site.
            return Ok(function);
        }
    };
    Ok(Expr { kind, ty, span })
}
