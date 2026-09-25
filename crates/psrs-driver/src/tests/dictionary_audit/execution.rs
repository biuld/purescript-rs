//! Value-sensitive execution and structural evidence for dictionary lowering.

use super::fixtures::*;
use super::{compile_fixture, execute_wasm, expect_fixture_exit};
use psrs_backend::mir::Instruction;
use psrs_hir::SymbolId;
use psrs_thir as thir;

fn pre_optimization_mir(
    name: &str,
    fixture: thir::Module,
    entry: SymbolId,
) -> psrs_backend::mir::Module {
    let mut core = psrs_core::lower_module(fixture)
        .unwrap_or_else(|errors| panic!("`{name}` Typed Core should lower: {errors:?}"));
    core.entry = Some(entry);
    let backend_input = psrs_backend::cc::lower_module(core)
        .unwrap_or_else(|errors| panic!("`{name}` should lower to CC: {errors:?}"));
    psrs_backend::mir::lower_module_with_bindings(
        backend_input.cc,
        backend_input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap_or_else(|errors| panic!("`{name}` should lower to MIR: {errors:?}"))
    .0
}

/// DICT-01, DICT-02, DICT-04, DICT-05: a `Global` `Eq` dictionary feeds a
/// contextual `Ord` instance, and a method selected through the superclass
/// field executes.
#[test]
fn typed_core_instance_and_superclass_evidence_execute() {
    let (fixture, entry) = dictionary_module();
    expect_fixture_exit("instance_superclass_evidence", fixture, entry, 42);
}

/// DICT-04: an instance method closure that captures its context dictionary
/// escapes through the product field and still executes.
#[test]
fn escaping_method_closure_capturing_context_executes() {
    let (fixture, entry) = escaping_method_module();
    expect_fixture_exit("escaping_method_closure", fixture, entry, 42);
}

/// DICT-08: a dictionary crossing an erased polymorphic identity recovers to
/// its concrete layout before method selection.
#[test]
fn dictionary_through_erased_polymorphism_executes() {
    let (fixture, entry) = erased_dictionary_module();
    expect_fixture_exit("erased_dictionary_round_trip", fixture, entry, 42);
}

/// DICT-08: a generic method (`forall a. a -> a`) stored in a dictionary field
/// is erased on storage and adapted back at its concrete use.
#[test]
fn polymorphic_method_field_executes() {
    let (fixture, entry) = polymorphic_method_module();
    expect_fixture_exit("polymorphic_method_field", fixture, entry, 42);
}

/// DICT-07: a class default stored in a method field is an ordinary closure and
/// executes like any other method.
#[test]
fn default_method_field_executes() {
    let (fixture, entry) = default_method_module();
    expect_fixture_exit("default_method_field", fixture, entry, 42);
}

/// DICT-07: a recursive instance constructor builds guarded dictionaries and
/// delegates to its captured context at the base case.
#[test]
fn recursive_instance_context_executes() {
    let (fixture, entry) = recursive_instance_module();
    expect_fixture_exit("recursive_instance_context", fixture, entry, 42);
}

/// DICT-03: a constrained binding lowers to a function whose first parameters
/// are its dictionaries, in declared order, before the ordinary argument.
#[test]
fn constrained_dictionary_parameters_precede_ordinary_arguments() {
    let (fixture, entry) = ordered_dictionaries_module();
    let mut core = psrs_core::lower_module(fixture).expect("ordered dictionary fixture lowers");
    core.entry = Some(entry);
    let declaration = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "constrained")
        .expect("constrained declaration");
    let mut binders = Vec::new();
    let mut value = &declaration.value;
    while let psrs_core::ExprKind::Lambda { binder, body } = &value.kind {
        binders.push(binder.name.clone());
        value = body;
    }
    assert_eq!(
        binders,
        vec!["da".to_string(), "db".to_string(), "x".to_string()],
        "dictionary parameters must lead the ordinary argument"
    );

    let (fixture, entry) = ordered_dictionaries_module();
    let stages = compile_fixture("ordered_dictionaries", fixture, entry);
    let cc_function = stages
        .cc
        .functions
        .iter()
        .find(|function| function.name == "constrained")
        .expect("CC constrained function");
    let parameter_shapes = cc_function
        .parameters
        .iter()
        .map(|parameter| {
            cc_function
                .values
                .iter()
                .find(|value| value.id == *parameter)
                .expect("declared parameter value")
                .ty
        })
        .collect::<Vec<_>>();
    assert_eq!(parameter_shapes.len(), 3);
    assert!(
        parameter_shapes[..2]
            .iter()
            .all(|shape| matches!(shape, psrs_backend::cc::ValueShape::Reference(_))),
        "the first two runtime parameters must be dictionary references: {parameter_shapes:?}"
    );
    assert_eq!(
        parameter_shapes[2],
        psrs_backend::cc::ValueShape::Integer,
        "the ordinary argument must follow the dictionaries"
    );
    let mir_function = stages
        .mir
        .functions
        .iter()
        .find(|function| function.name == "constrained")
        .expect("MIR constrained function");
    assert_eq!(mir_function.parameters.len(), 3);

    let (fixture, entry) = ordered_dictionaries_module();
    expect_fixture_exit("ordered_dictionaries", fixture, entry, 42);
}

/// DICT-06, DICT-10: a `let`-bound dictionary is constructed once in the
/// unspecialized path, and the optimized artifact preserves the result.
#[test]
fn shared_dictionary_binds_once_and_optimizes_safely() {
    let (fixture, entry) = shared_dictionary_module();
    let mir = pre_optimization_mir("shared_dictionary", fixture, entry);
    let main = mir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let instructions = main
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .collect::<Vec<_>>();
    let constructor = psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 1);
    let constructions = instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction,
                Instruction::Call { function, .. } if *function == constructor
            )
        })
        .count();
    let projected = instructions
        .iter()
        .filter(|instruction| matches!(instruction, Instruction::StructGet { .. }))
        .count();
    assert_eq!(
        constructions, 1,
        "a let-bound instance dictionary must be constructed exactly once"
    );
    assert!(
        projected >= 2,
        "both dictionary uses must project from the shared value"
    );

    let (fixture, entry) = shared_dictionary_module();
    let stages = compile_fixture("shared_dictionary", fixture, entry);
    assert!(
        stages
            .mir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction, Instruction::StructGet { .. })),
        "the optimized dictionary path must still project product fields"
    );
    match execute_wasm("shared_dictionary", &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(output.status.code(), Some(42)),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}
