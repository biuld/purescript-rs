use super::*;
use psrs_core::Module as CoreModule;
use psrs_hir::{ExternalKind, ExternalSymbol, ModuleId, SymbolId};
use psrs_span::TextRange;

fn symbol(index: u32) -> SymbolId {
    SymbolId::new(ModuleId::INTRINSICS, index)
}

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn wit_external(symbol: SymbolId) -> ExternalSymbol {
    ExternalSymbol {
        symbol,
        name: "log".into(),
        kind: ExternalKind::Wit {
            interface: "wasi:cli/stdout@0.2.12".into(),
            function: "log".into(),
        },
        signature: None,
    }
}

fn binding(symbol: SymbolId) -> ExternalBinding {
    ExternalBinding {
        symbol,
        interface: "wasi:cli/stdout@0.2.12".into(),
        function: "log".into(),
        signature: None,
    }
}

fn module(externals: Vec<ExternalSymbol>) -> CoreModule {
    CoreModule {
        id: ModuleId(1),
        name: "Main".into(),
        externals,
        types: Vec::new(),
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    }
}

fn errors_contain(errors: &[BackendError], fragment: &str) -> bool {
    errors.iter().any(|error| error.message.contains(fragment))
}

#[test]
fn accepts_a_complete_wit_projection() {
    let external = symbol(1);
    let bindings = ExternalBindings {
        imports: vec![binding(external)],
    };
    assert!(
        bindings
            .validate_core(&module(vec![wit_external(external)]))
            .is_ok()
    );
}

#[test]
fn rejects_a_missing_wit_binding() {
    let external = symbol(1);
    let bindings = ExternalBindings::default();
    let errors = bindings
        .validate_core(&module(vec![wit_external(external)]))
        .expect_err("a source WIT import without a binding must be rejected");
    assert!(errors_contain(&errors, "has no external binding"));
}

#[test]
fn rejects_a_duplicate_binding() {
    let external = symbol(1);
    let bindings = ExternalBindings {
        imports: vec![binding(external), binding(external)],
    };
    let errors = bindings
        .validate_core(&module(vec![wit_external(external)]))
        .expect_err("a duplicated binding must be rejected");
    assert!(errors_contain(&errors, "is duplicated"));
}

#[test]
fn rejects_a_binding_for_a_non_source_import() {
    let external = symbol(1);
    let bindings = ExternalBindings {
        imports: vec![binding(external)],
    };
    let errors = bindings
        .validate_core(&module(Vec::new()))
        .expect_err("a binding without a source WIT import must be rejected");
    assert!(errors_contain(&errors, "is not a source WIT import"));
}
