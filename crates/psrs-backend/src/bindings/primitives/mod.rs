//! Discharges explicit primitive bindings into verified ordinary Core functions.

use crate::BackendError;
use psrs_core::{Binder, Declaration, Expr, ExprKind, Module, arrow_parts, scheme_parts};
use psrs_hir::{ExternalKind, Intrinsic, IntrinsicCategory, LocalId};

#[cfg(test)]
mod tests;

pub(crate) fn lower(module: &mut Module, source: Option<&Module>) -> Result<(), Vec<BackendError>> {
    let bindings = module
        .externals
        .iter()
        .filter_map(|external| {
            let ExternalKind::Primitive(intrinsic) = external.kind else {
                return None;
            };
            Some((external.clone(), intrinsic))
        })
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        return Ok(());
    }
    module
        .verify_with_source(source.unwrap_or(module))
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| {
                    BackendError::invalid_ir("P8 primitive linking", error.span, error.message)
                        .with_module(error.module)
                })
                .collect::<Vec<_>>()
        })?;
    // Work on a complete candidate. Failed validation must not publish a
    // partially discharged external table or generated declaration.
    let mut candidate = module.clone();
    // Each generated function is closed over only its typed parameters and
    // the registry operation. Validate these independent units with the full
    // type/representation facts, without rechecking every source body for
    // each binding. The common CC entry validates the complete output module.
    let mut probe = module.clone();
    probe.declarations.clear();
    probe.entry = None;
    let operations = bindings
        .iter()
        .map(|(external, intrinsic)| (external.symbol, *intrinsic))
        .collect();
    for (external, intrinsic) in bindings {
        let span = external
            .signature
            .as_ref()
            .map_or(module.span, |ty| ty.span);
        let Some(checked) = module
            .external_types
            .iter()
            .find(|ty| ty.symbol == external.symbol)
        else {
            return Err(vec![BackendError::new(
                "P8 primitive linking",
                span,
                "primitive binding has no checked source signature",
            )]);
        };
        let error = |message: String| {
            vec![
                BackendError::new("P8 primitive linking", span, message)
                    .with_module(checked.source_module),
            ]
        };
        if !matches!(
            intrinsic.descriptor().category,
            IntrinsicCategory::Unary
                | IntrinsicCategory::BinaryScalar
                | IntrinsicCategory::ArrayLength
                | IntrinsicCategory::ArrayIndex
                | IntrinsicCategory::ArrayUpdate
                | IntrinsicCategory::ArrayAppend
                | IntrinsicCategory::ArrayFill
                | IntrinsicCategory::ArrayWrite
                | IntrinsicCategory::StringToBytes
                | IntrinsicCategory::BytesToString
        ) && intrinsic != Intrinsic::UnsafeCoerce
        {
            return Err(error(format!(
                "primitive binding `{}` has no foreign-function implementation yet",
                intrinsic.descriptor().name
            )));
        }
        let Some((quantified, body_type)) = scheme_parts(&candidate.types, checked.ty) else {
            return Err(error(
                "primitive binding has an invalid checked scheme".into(),
            ));
        };
        let mut result = body_type;
        let mut parameters = Vec::new();
        let mut arrows = Vec::new();
        while let Some((parameter, tail)) = arrow_parts(&candidate.types, result) {
            arrows.push(result);
            parameters.push(Binder {
                id: LocalId(parameters.len() as u32),
                name: format!("primitive_argument_{}", parameters.len()),
                ty: parameter,
                span,
            });
            result = tail;
        }
        if parameters.len() != intrinsic.descriptor().arity as usize {
            return Err(error(format!(
                "primitive binding `{}` does not match its declared function type",
                intrinsic.descriptor().name
            )));
        }
        let mut value = psrs_core::primitive::primitive_value(
            intrinsic,
            parameters
                .iter()
                .map(|binder| Expr {
                    kind: ExprKind::Local(binder.id),
                    ty: binder.ty,
                    span,
                })
                .collect(),
            result,
            span,
        );
        for (binder, ty) in parameters.into_iter().zip(arrows).rev() {
            value = Expr {
                kind: ExprKind::Lambda {
                    binder,
                    body: Box::new(value),
                },
                ty,
                span,
            };
        }
        let declaration = Declaration {
            symbol: external.symbol,
            name: external.name,
            name_span: span,
            quantified,
            ty: body_type,
            value,
            span,
        };
        probe.declarations.push(declaration.clone());
        candidate.declarations.push(declaration);
        candidate
            .externals
            .retain(|value| value.symbol != external.symbol);
        candidate
            .external_types
            .retain(|value| value.symbol != external.symbol);
        probe
            .externals
            .retain(|value| value.symbol != external.symbol);
        probe
            .external_types
            .retain(|value| value.symbol != external.symbol);
        // Core's existing intrinsic rules check operand and result identity,
        // including unused bindings. No source-type or arity heuristic is used.
        if let Err(errors) = probe.verify_with_source(source.unwrap_or(&probe)) {
            return Err(error(format!(
                "primitive binding `{}` violates its checked type contract: {}",
                intrinsic.descriptor().name,
                errors
                    .iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        probe.declarations.clear();
    }
    psrs_core::primitive::expand_primitive_globals(&mut candidate, &operations).map_err(
        |errors| {
            errors
                .into_iter()
                .map(|error| {
                    BackendError::invalid_ir("P8 primitive linking", error.span, error.message)
                        .with_module(error.module)
                })
                .collect::<Vec<_>>()
        },
    )?;
    candidate
        .verify_with_source(source.unwrap_or(&candidate))
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| {
                    BackendError::invalid_ir("P8 primitive linking", error.span, error.message)
                        .with_module(error.module)
                })
                .collect::<Vec<_>>()
        })?;
    *module = candidate;
    Ok(())
}
