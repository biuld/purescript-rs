use psrs_hir::{ExternalKind, ExternalSymbol, Intrinsic};

/// The compiler-known externals available to every bootstrap module.
///
/// The name, arity, category, and type of each intrinsic come from its
/// descriptor in `psrs-hir`, so this table is not a second source of truth.
/// `Intrinsic::ALL` is kept exact by a compile-time assertion, so every variant
/// is bootstrapped.
pub fn bootstrap_externals() -> Vec<ExternalSymbol> {
    Intrinsic::ALL
        .into_iter()
        .map(|intrinsic| {
            let descriptor = intrinsic.descriptor();
            ExternalSymbol {
                symbol: intrinsic.symbol(),
                name: descriptor.name.to_owned(),
                kind: ExternalKind::Intrinsic(intrinsic),
                signature: Some((descriptor.scheme)()),
            }
        })
        .collect()
}
