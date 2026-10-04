use crate::{ExternalType, Module, Type, TypeConstructor, TypeId};
use psrs_hir::{ExternalKind, ExternalSymbol, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

fn fixture() -> Module {
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    Module {
        id: ModuleId(2),
        name: "ExternalScheme".into(),
        externals: vec![ExternalSymbol {
            symbol,
            name: "clock".into(),
            kind: ExternalKind::Wit {
                interface: "wasi:clocks/monotonic-clock".into(),
                function: "now".into(),
            },
            signature: None,
        }],
        external_types: vec![ExternalType {
            symbol,
            source_module: ModuleId(2),
            ty: TypeId(0),
        }],
        types: vec![Type::Constructor(TypeConstructor::Int)],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        type_names: Vec::new(),
        span: TextRange::new(0, 5),
    }
}

fn rejects(module: &Module, fragment: &str) {
    let errors = module
        .verify()
        .expect_err("malformed checked external scheme must fail");
    assert!(
        errors.iter().any(|error| error.message.contains(fragment)),
        "{errors:?}"
    );
}

#[test]
fn a_wit_scheme_is_required_even_without_a_raw_annotation() {
    let mut module = fixture();
    module.verify().unwrap();
    module.external_types.clear();
    rejects(&module, "no checked signature");
}

#[test]
fn checked_external_schemes_are_unique_and_have_an_external_owner() {
    let mut module = fixture();
    module.external_types.push(module.external_types[0].clone());
    rejects(&module, "more than one checked signature");
    let mut module = fixture();
    module.externals.clear();
    rejects(&module, "no external declaration");
}

#[test]
fn checked_external_schemes_reject_invalid_type_references() {
    let mut module = fixture();
    module.external_types[0].ty = TypeId(99);
    rejects(&module, "type");
}

#[test]
fn external_quantifiers_scope_their_variables() {
    let mut module = fixture();
    let variable = TypeVariableId(7);
    module.types = vec![Type::Variable(variable)];
    rejects(&module, "quantifier scope");
    module.types.push(Type::ForAll {
        variables: vec![variable],
        body: TypeId(0),
    });
    module.external_types[0].ty = TypeId(1);
    module
        .verify()
        .expect("a properly quantified external scheme is valid");
    module.types[1] = Type::ForAll {
        variables: vec![variable, variable],
        body: TypeId(0),
    };
    rejects(&module, "unique");
}

#[test]
fn cyclic_external_schemes_are_rejected() {
    let mut module = fixture();
    module.types[0] = Type::ForAll {
        variables: Vec::new(),
        body: TypeId(0),
    };
    rejects(&module, "cycle");
}
