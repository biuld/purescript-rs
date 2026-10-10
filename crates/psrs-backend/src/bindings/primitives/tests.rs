use super::*;
use psrs_core::{ExternalType, Type, TypeConstructor, TypeId};
use psrs_hir::{ExternalSymbol, Intrinsic, ModuleId, SymbolId};
use psrs_span::TextRange;

fn fixture(types: &[TypeId]) -> Module {
    let span = TextRange::new(0, 1);
    let externals = types
        .iter()
        .enumerate()
        .map(|(index, _)| ExternalSymbol {
            symbol: SymbolId::new(ModuleId::INTRINSICS, 200 + index as u32),
            name: format!("convert{index}"),
            kind: ExternalKind::Primitive(Intrinsic::IntToNumber),
            signature: None,
        })
        .collect::<Vec<_>>();
    let external_types = externals
        .iter()
        .zip(types)
        .map(|(external, ty)| ExternalType {
            symbol: external.symbol,
            source_module: ModuleId(7),
            ty: *ty,
        })
        .collect();
    Module {
        id: ModuleId(7),
        name: "Bindings".into(),
        externals,
        external_types,
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Number),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(2), TypeId(0)),
            Type::Application(TypeId(3), TypeId(1)),
            Type::Application(TypeId(3), TypeId(0)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        type_names: Vec::new(),
        entry: None,
        span,
    }
}

#[test]
fn primitive_linking_does_not_publish_an_earlier_binding_when_a_later_one_fails() {
    let mut module = fixture(&[TypeId(4), TypeId(5)]);
    let before = module.clone();
    let errors = lower(&mut module, None).expect_err("second source signature is wrong");
    assert_eq!(module, before);
    assert!(errors.iter().all(|error| error.module == Some(ModuleId(7))));
}

#[test]
fn primitive_linking_rejects_cyclic_checked_input_before_traversing_arrows() {
    let mut module = fixture(&[TypeId(4)]);
    module.types[4] = Type::Application(TypeId(3), TypeId(4));
    let before = module.clone();
    let errors = lower(&mut module, None).expect_err("cyclic source scheme is invalid IR");
    assert_eq!(module, before);
    assert!(
        errors
            .iter()
            .any(|error| error.kind == crate::BackendErrorKind::InvalidCompilerIr)
    );
}

#[test]
fn primitive_linking_transfers_checked_identity_to_a_verified_function() {
    let mut module = fixture(&[TypeId(4)]);
    let symbol = module.externals[0].symbol;
    lower(&mut module, None).unwrap();
    assert!(module.externals.is_empty());
    assert!(module.external_types.is_empty());
    assert_eq!(module.declarations[0].symbol, symbol);
    assert_eq!(module.declarations[0].ty, TypeId(4));
    module.verify().unwrap();
}
