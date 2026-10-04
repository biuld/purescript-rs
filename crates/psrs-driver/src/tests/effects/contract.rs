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

#[test]
fn checked_import_verification_keeps_the_foreign_source_module() {
    let prepared = crate::prepare_sources(&[
        (
            "Clock.purs",
            "module Clock where\nimport Prelude\nforeign import \"wasi:clocks/monotonic-clock#now\" keep :: Int\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Prelude\nimport Clock\nmain = keep\n",
        ),
    ])
    .expect("the clock import links before its checked signature is damaged");
    let mut core = prepared.core;
    let context = prepared.effect_context.expect("trusted effect");
    let entry = core.entry.expect("selected main");
    let keep = core
        .externals
        .iter()
        .find(|external| external.name == "keep")
        .expect("clock import")
        .symbol;
    let source_module = core
        .external_types
        .iter()
        .find(|external| external.symbol == keep)
        .expect("checked clock signature")
        .source_module;
    assert_ne!(source_module, entry.module);
    core.types
        .push(psrs_core::Type::Variable(psrs_hir::TypeVariableId(900_001)));
    let free = psrs_core::TypeId((core.types.len() - 1) as u32);
    core.external_types
        .iter_mut()
        .find(|external| external.symbol == keep)
        .expect("checked clock signature")
        .ty = free;
    let context = Some(context);
    let optimized = psrs_backend::compile_with_context(
        core.clone(),
        context.clone(),
        psrs_backend::TargetCapabilities::default(),
    )
    .expect_err("optimization must reject an unbound variable in a checked import");
    let lowered = psrs_backend::lower_cc_with_context(core, context.as_ref())
        .expect_err("effect lowering must reject an unbound variable in a checked import");
    for errors in [optimized, lowered] {
        assert!(
            errors.iter().any(|error| {
                error.module == Some(source_module)
                    && error
                        .message
                        .contains("type variable is outside its quantifier scope")
            }),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .all(|error| error.module != Some(entry.module)),
            "the entry module must not own the import's verification failure: {errors:?}"
        );
    }
}
