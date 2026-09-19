use psrs_hir::{ExternalKind, ExternalSymbol, Intrinsic, host_functions};

/// The compiler-known externals available to every bootstrap module.
pub fn bootstrap_externals() -> Vec<ExternalSymbol> {
    let intrinsics = [
        ("true", Intrinsic::BoolTrue),
        ("false", Intrinsic::BoolFalse),
        ("+", Intrinsic::I32Add),
        ("-", Intrinsic::I32Sub),
        ("*", Intrinsic::I32Mul),
        ("/", Intrinsic::I32DivS),
        ("%", Intrinsic::I32RemS),
        ("==", Intrinsic::I32Eq),
        ("/=", Intrinsic::I32Ne),
        ("<", Intrinsic::I32LtS),
        ("<=", Intrinsic::I32LeS),
        (">", Intrinsic::I32GtS),
        (">=", Intrinsic::I32GeS),
    ]
    .into_iter()
    .map(|(name, intrinsic)| ExternalSymbol {
        symbol: intrinsic.symbol(),
        name: name.into(),
        kind: ExternalKind::Intrinsic(intrinsic),
    });
    let host = host_functions().into_iter().map(|function| ExternalSymbol {
        symbol: function.symbol,
        name: function.name.into(),
        kind: ExternalKind::Host,
    });
    intrinsics.chain(host).collect()
}
