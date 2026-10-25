//! Checked command entry adaptation through ordinary library functions.
use crate::{
    Declaration, Expr, ExprKind, Module, Type, TypeConstructor, TypeId, VerifyError, arrow_parts,
};
use psrs_hir::SymbolId;

/// Normalize a selected entry with a source runner instantiable at `T -> Int`.
/// Both identities must have been resolved by the caller. This neither names
/// an effect type nor synthesizes its operations. Failure leaves Core unchanged.
pub fn normalize_entry(
    module: &mut Module,
    entry: SymbolId,
    runner: SymbolId,
) -> Result<SymbolId, Vec<VerifyError>> {
    module.verify()?;
    let source = module
        .declarations
        .iter()
        .find(|decl| decl.symbol == entry)
        .ok_or_else(|| {
            error(
                module,
                entry,
                "selected command entry has no source declaration",
            )
        })?;
    let runner_decl = module
        .declarations
        .iter()
        .find(|decl| decl.symbol == runner)
        .ok_or_else(|| {
            error(
                module,
                entry,
                "configured command runner has no source declaration",
            )
        })?;
    let (_, result) = arrow_parts(&module.types, runner_decl.ty).ok_or_else(|| {
        error(
            module,
            runner,
            "configured command runner must have type T -> Int",
        )
    })?;
    if module.types.get(result.0 as usize) != Some(&Type::Constructor(TypeConstructor::Int)) {
        return Err(error(
            module,
            runner,
            "configured command runner must return Int",
        ));
    }
    if !crate::types::free_variables(module, source.ty).is_empty() {
        return Err(error(module, entry, "command entry must be monomorphic"));
    }
    if module.types.get(source.ty.0 as usize) == Some(&Type::Constructor(TypeConstructor::Int)) {
        module.entry = Some(entry);
        return Ok(entry);
    }
    // Check the entire callable instance with Core's authoritative scheme
    // relation. Repeated variables, row tails and nested quantifiers must obey
    // the same rules as an ordinary source global; this is not an Effect rule.
    let mut candidate = module.clone();
    let function = intern(
        &mut candidate.types,
        Type::Constructor(TypeConstructor::Function),
    );
    let parameter = intern(&mut candidate.types, Type::Application(function, source.ty));
    let runner_type = intern(&mut candidate.types, Type::Application(parameter, result));
    if candidate
        .checked_instantiation(runner_decl.ty, &runner_decl.quantified, runner_type)
        .is_none()
    {
        return Err(error(
            module,
            entry,
            "command entry type disagrees with the configured runner input",
        ));
    }
    let index = module
        .declarations
        .iter()
        .map(|decl| decl.symbol)
        .chain(module.externals.iter().map(|external| external.symbol))
        .chain(
            module
                .constructors
                .iter()
                .map(|constructor| constructor.symbol),
        )
        .filter(|symbol| symbol.module == entry.module)
        .map(|symbol| symbol.index)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| error(module, entry, "command entry symbol space is exhausted"))?;
    let symbol = SymbolId::new(entry.module, index);
    let span = source.name_span;
    let global = |symbol, ty| Expr {
        kind: ExprKind::Global(symbol),
        ty,
        span,
    };
    let wrapper = Declaration {
        symbol,
        name: "$command_entry".into(),
        name_span: span,
        quantified: vec![],
        ty: result,
        value: Expr {
            kind: ExprKind::Application(
                Box::new(global(runner, runner_type)),
                Box::new(global(entry, source.ty)),
            ),
            ty: result,
            span,
        },
        span,
    };
    candidate.declarations.push(wrapper);
    candidate.entry = Some(symbol);
    candidate.verify()?;
    *module = candidate;
    Ok(symbol)
}

fn intern(types: &mut Vec<Type>, ty: Type) -> TypeId {
    if let Some(index) = types.iter().position(|existing| *existing == ty) {
        return TypeId(index as u32);
    }
    let id = TypeId(types.len() as u32);
    types.push(ty);
    id
}

fn error(module: &Module, symbol: SymbolId, message: &'static str) -> Vec<VerifyError> {
    vec![VerifyError::new(
        symbol.module,
        module
            .declarations
            .iter()
            .find(|decl| decl.symbol == symbol)
            .map_or(module.span, |decl| decl.name_span),
        message,
    )]
}
