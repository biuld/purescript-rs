use super::*;

#[test]
fn maps_generic_arrays_across_polymorphic_adt_boundaries() {
    let source = "\
module Main where
data Wrap a = Wrap (Array a)
wrap :: forall a. Array a -> Wrap a
wrap values = Wrap values
unwrap :: forall a. Wrap a -> Array a
unwrap value = case value of
  Wrap values -> values
main = arrayIndex (unwrap (wrap [40, 42])) 1
";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let generic_cc = psrs_backend::cc::lower_module(core.clone())
        .expect("generic source Core should lower to CC before P7")
        .cc;
    let generic_array_maps = aggregate_conversions(&generic_cc)
        .iter()
        .map(|conversion| array_map_count(&conversion.plan))
        .sum::<usize>();
    assert!(
        generic_array_maps >= 2,
        "expected pre-P7 concrete/generic boundary maps; found {generic_array_maps}"
    );
    let stages = psrs_backend::compile_with_stages(core)
        .expect("generic Array a must map at concrete call boundaries");
    let erased = psrs_backend::cc::ValueShape::Reference(psrs_backend::cc::Reference {
        nullable: false,
        heap: psrs_backend::cc::RefShape::Erased,
    });
    let generic_array = stages
        .cc
        .representations
        .representations
        .iter()
        .position(|representation| {
            matches!(representation, psrs_backend::cc::Representation::Array { element } if *element == erased)
        })
        .map(|index| psrs_backend::cc::ReprId(index as u32))
        .expect("the generic array layout should be canonical");
    assert!(matches!(
        stages
            .cc
            .representations
            .representations
            .get(generic_array.0 as usize),
        Some(psrs_backend::cc::Representation::Array { element }) if *element == erased
    ));
    assert!(
        stages
            .cc
            .representations
            .representations
            .iter()
            .any(|representation| {
                matches!(representation, psrs_backend::cc::Representation::Variant { cases }
            if cases.iter().any(|case| case.fields.contains(&erased)))
            })
    );
    let array_maps = stages
        .mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(
                instruction,
                psrs_backend::mir::Instruction::ArrayNewDefault { .. }
            )
        })
        .count();
    assert!(
        array_maps >= 2,
        "expected specialization and canonicalization maps; found {array_maps}"
    );
    assert!(stages.artifact.wat.contains("array.new_default"));
    assert!(stages.artifact.wat.contains("array.get"));
    assert!(stages.artifact.wat.contains("array.set"));
    let Some(output) = run_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "wasmtime failed: {output:?}"
    );
}

#[test]
fn maps_closed_generic_records_and_nested_arrays_across_instantiations() {
    let source = "\
module Main where
copy :: forall a. { items :: Array a, value :: a } -> { items :: Array a, value :: a }
copy record = record { value = record.value }
main = arrayIndex ((copy { items: [40, 42], value: 7 }).items) 1
";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let generic_cc = psrs_backend::cc::lower_module(core.clone())
        .expect("generic source Core should lower to CC before P7")
        .cc;
    let generic_conversions = aggregate_conversions(&generic_cc);
    assert!(
        generic_conversions
            .iter()
            .any(|conversion| { contains_canonical_record_array_map(&conversion.plan) }),
        "expected a pre-P7 canonical closed-record map containing a nested array map"
    );
    let stages = psrs_backend::compile_with_stages(core)
        .expect("closed generic records should map across concrete instantiations");
    assert!(stages.artifact.wat.contains("struct.new"));
    assert!(stages.artifact.wat.contains("struct.get"));
    assert!(stages.artifact.wat.contains("array.new_fixed"));
    assert!(stages.artifact.wat.contains("array.get"));
    let Some(output) = run_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "wasmtime failed: {output:?}"
    );
}

#[test]
fn maps_nested_generic_arrays_recursively_across_instantiations() {
    let source = "\
module Main where
duplicate :: forall a. Array (Array a) -> Array (Array a)
duplicate values = values
main = arrayIndex (arrayIndex (duplicate [[40, 42]]) 0) 1
";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let generic_cc = psrs_backend::cc::lower_module(core.clone())
        .expect("generic source Core should lower to CC before P7")
        .cc;
    let conversions = aggregate_conversions(&generic_cc);
    let maximum_nested_array_maps = conversions
        .iter()
        .map(|conversion| array_map_depth(&conversion.plan))
        .max()
        .unwrap_or_default();
    assert!(
        maximum_nested_array_maps >= 2,
        "expected pre-P7 recursively nested ArrayMap plans; found depth {maximum_nested_array_maps}"
    );
    let stages = psrs_backend::compile_with_stages(core)
        .expect("nested generic arrays should map recursively");
    assert!(stages.artifact.wat.contains("array.get"));
    let Some(output) = run_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "wasmtime failed: {output:?}"
    );
}

#[test]
fn adapts_higher_order_array_arguments_and_results() {
    let source = "\
module Main where
applyArray :: forall a. (Array a -> Array a) -> Array a -> Array a
applyArray function values = function values
concrete :: Array Int -> Array Int
concrete values = values
main = arrayIndex (applyArray concrete [40, 42]) 1
";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let generic_cc = psrs_backend::cc::lower_module(core.clone())
        .expect("generic source Core should lower to CC before P7")
        .cc;
    let array_maps = aggregate_conversions(&generic_cc)
        .iter()
        .map(|conversion| array_map_count(&conversion.plan))
        .sum::<usize>();
    assert!(
        array_maps >= 2,
        "expected pre-P7 higher-order argument and result ArrayMap plans; found {array_maps}"
    );
    let stages = psrs_backend::compile_with_stages(core)
        .expect("higher-order adapters should convert generic aggregate arguments and results");
    assert!(stages.artifact.wat.contains("array.get"));
    let Some(output) = run_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "wasmtime failed: {output:?}"
    );
}

fn aggregate_conversions(
    module: &psrs_backend::cc::Module,
) -> Vec<&psrs_backend::cc::AggregateConvert> {
    fn collect<'a>(
        assignments: &'a [psrs_backend::cc::Assignment],
        conversions: &mut Vec<&'a psrs_backend::cc::AggregateConvert>,
    ) {
        use psrs_backend::cc::AssignmentKind;

        for assignment in assignments {
            match &assignment.kind {
                AssignmentKind::AggregateConvert { conversion, .. } => {
                    conversions.push(conversion);
                }
                AssignmentKind::If {
                    then_assignments,
                    else_assignments,
                    ..
                } => {
                    collect(then_assignments, conversions);
                    collect(else_assignments, conversions);
                }
                AssignmentKind::TagSwitch {
                    cases,
                    default_assignments,
                    ..
                } => {
                    for case in cases {
                        collect(&case.assignments, conversions);
                    }
                    collect(default_assignments, conversions);
                }
                _ => {}
            }
        }
    }

    let mut conversions = Vec::new();
    for function in &module.functions {
        collect(&function.assignments, &mut conversions);
    }
    conversions
}

fn array_map_count(conversion: &psrs_backend::cc::ValueConversion) -> usize {
    use psrs_backend::cc::ValueConversion;

    match conversion {
        ValueConversion::ArrayMap { element, .. } => 1 + array_map_count(element),
        ValueConversion::ProductMap { fields, .. } => fields.iter().map(array_map_count).sum(),
        ValueConversion::Sequence(steps) => steps.iter().map(array_map_count).sum(),
        _ => 0,
    }
}

fn array_map_depth(conversion: &psrs_backend::cc::ValueConversion) -> usize {
    use psrs_backend::cc::ValueConversion;

    match conversion {
        ValueConversion::ArrayMap { element, .. } => 1 + array_map_depth(element),
        ValueConversion::ProductMap { fields, .. } | ValueConversion::Sequence(fields) => {
            fields.iter().map(array_map_depth).max().unwrap_or_default()
        }
        _ => 0,
    }
}

fn contains_canonical_record_array_map(conversion: &psrs_backend::cc::ValueConversion) -> bool {
    use psrs_backend::cc::ValueConversion;

    match conversion {
        ValueConversion::ProductMap { labels, fields, .. } => {
            labels == &["items".to_owned(), "value".to_owned()]
                && fields.iter().any(|field| array_map_count(field) > 0)
        }
        ValueConversion::ArrayMap { element, .. } => contains_canonical_record_array_map(element),
        ValueConversion::Sequence(steps) => steps.iter().any(contains_canonical_record_array_map),
        _ => false,
    }
}

fn run_wasmtime(source: &str) -> Option<std::process::Output> {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_source("Main.purs", source).unwrap();
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-parameterized-shapes-{}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, &artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Some(output)
}
