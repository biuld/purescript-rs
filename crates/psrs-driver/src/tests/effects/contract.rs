use psrs_core::effect::{EffectCommandEntry, EffectCompilation, EffectOperation};

fn fixture() -> (psrs_core::Module, EffectCompilation) {
    let lowered = crate::lower_source_with_prelude_to_core(
        "Main.purs",
        "module Main where\nimport Prelude\nmain = 0\n",
    )
    .unwrap();
    let core = lowered.core;
    let context = lowered.effect_context;
    (
        core,
        context.expect("embedded Prelude supplies the trusted contract"),
    )
}

fn rejects(core: psrs_core::Module, context: EffectCompilation, message: &str) {
    let errors = psrs_backend::compile_with_context(
        core,
        Some(context),
        psrs_backend::TargetCapabilities::default(),
    )
    .expect_err("a malformed explicit Effect contract must fail before encoding");
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{errors:?}"
    );
}

#[test]
fn the_backend_rejects_partial_and_duplicate_trusted_operation_metadata() {
    let (core, mut context) = fixture();
    context
        .trusted
        .operations
        .retain(|binding| binding.operation != EffectOperation::Trap);
    rejects(core, context, "missing a required operation binding");

    let (core, mut context) = fixture();
    context
        .trusted
        .operations
        .push(context.trusted.operations[0]);
    rejects(core, context, "duplicated");
}

#[test]
fn the_backend_rejects_operation_identity_and_checked_signature_mismatches() {
    let (core, mut context) = fixture();
    context.trusted.operations[0].symbol = core.entry.unwrap();
    rejects(core, context, "no external declaration");

    let (mut core, context) = fixture();
    let pure = context
        .trusted
        .operations
        .iter()
        .find(|binding| binding.operation == EffectOperation::Pure)
        .unwrap()
        .symbol;
    let integer = psrs_core::TypeId(
        core.types
            .iter()
            .position(|ty| {
                matches!(
                    ty,
                    psrs_core::Type::Constructor(psrs_core::TypeConstructor::Int)
                )
            })
            .unwrap() as u32,
    );
    core.external_types
        .iter_mut()
        .find(|external| external.symbol == pure)
        .unwrap()
        .ty = integer;
    rejects(core, context, "invalid checked source signature");
}

#[test]
fn the_backend_rejects_an_effect_entry_context_for_an_integer_source_entry() {
    let (core, mut context) = fixture();
    context.command_entry = Some(EffectCommandEntry {
        symbol: core.entry.unwrap(),
    });
    rejects(core, context, "must have type Effect Unit");
}

fn action_fixture() -> (psrs_core::Module, EffectCompilation) {
    let lowered = crate::lower_source_with_prelude_to_core(
        "Main.purs",
        "module Main where\nimport Prelude\nmain :: Effect Unit\nmain = pure unit\n",
    )
    .unwrap();
    (lowered.core, lowered.effect_context.unwrap())
}

#[test]
fn effect_command_metadata_must_name_the_selected_source_entry() {
    let (mut core, mut context) = action_fixture();
    let selected = core.entry.unwrap();
    let mut decoy = core
        .declarations
        .iter()
        .find(|decl| decl.symbol == selected)
        .unwrap()
        .clone();
    decoy.symbol = psrs_hir::SymbolId::new(selected.module, 1_000_000);
    decoy.name = "decoy".into();
    context.command_entry = Some(EffectCommandEntry {
        symbol: decoy.symbol,
    });
    core.declarations.push(decoy);
    rejects(core, context, "selected source entry");
}

#[test]
fn an_effect_source_entry_requires_command_metadata() {
    let (core, mut context) = action_fixture();
    context.command_entry = None;
    rejects(core, context, "missing Effect command entry");
}
