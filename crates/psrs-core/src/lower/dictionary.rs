use super::{Expr, ExprKind, LowerError, TypeId};
use psrs_thir::{Evidence, EvidenceKind, Type};

/// Erases checked class evidence into the existing Core value and call forms.
pub(super) fn lower_evidence(evidence: &Evidence, types: &[Type]) -> Result<Expr, LowerError> {
    let span = evidence.span;
    let ty = TypeId(evidence.ty.0);
    let kind = match &evidence.kind {
        EvidenceKind::Given(id) => ExprKind::Local(*id),
        EvidenceKind::Global(symbol) => ExprKind::Global(*symbol),
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
                let Some(Type::Function { parameter, result }) =
                    types.get(function_type.0 as usize)
                else {
                    return Err(LowerError {
                        span: evidence_argument.span,
                        message: "instance dictionary constructor takes too few context arguments",
                    });
                };
                let argument = lower_evidence(evidence_argument, types)?;
                if argument.ty != TypeId(parameter.0) {
                    return Err(LowerError {
                        span: evidence_argument.span,
                        message: "instance evidence does not match its context parameter",
                    });
                }
                let result = *result;
                function = Expr {
                    kind: ExprKind::Application(Box::new(function), Box::new(argument)),
                    ty: TypeId(result.0),
                    span,
                };
                function_type = result;
            }
            if function.ty != ty {
                return Err(LowerError {
                    span,
                    message: "instance evidence result has the wrong dictionary type",
                });
            }
            return Ok(function);
        }
    };
    Ok(Expr { kind, ty, span })
}
