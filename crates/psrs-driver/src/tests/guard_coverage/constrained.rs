use super::*;

#[test]
fn guarded_polymorphic_results_use_the_enclosing_dictionary() {
    let prefix = "module Main where\nclass Build f where\n  build :: forall a. a -> f a\ndata Box a = Box a\ninstance buildBox :: Build Box where\n  build = Box\nchoose :: forall f a. Build f => Boolean -> a -> f a\n";
    for (body, flag) in [
        (
            "choose = case _, _ of\n  flag, value\n    | flag -> build value\n    | true -> build value\n",
            "true",
        ),
        (
            "choose = case _, _ of\n  flag, value\n    | flag -> build value\n    | true -> build value\n",
            "false",
        ),
        (
            "choose flag value = case flag of\n  false -> build value\n  true | flag -> build value\n  true -> build value\n",
            "true",
        ),
        (
            "choose flag value = case flag of\n  true | false -> build value\n  _ -> build value\n",
            "true",
        ),
    ] {
        let source = format!(
            "{prefix}{body}main :: Int\nmain = case (choose {flag} 42 :: Box Int) of\n  Box value -> value\n"
        );
        let Some(output) = run_with_wasmtime(&source) else {
            eprintln!("skipping: wasmtime is not installed");
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{body}: {output:?}");
    }
}

#[test]
fn official_guarded_library_declarations_typecheck() {
    let source = "module Main where\nimport Data.Enum\nimport Data.Enum.Generic\nimport Data.List.Lazy\nmain :: Int\nmain = 42\n";
    check_program_types_lenient_with_prelude(&[("Main.purs", source)]).unwrap();
}

#[test]
fn official_enum_ranges_execute_ascending_descending_and_equal_guards() {
    let source = "module Main where\nimport Prelude\nimport Data.Enum (enumFromTo)\nmain :: Int\nmain = if enumFromTo 1 3 == [1,2,3] && enumFromTo 3 1 == [3,2,1] && enumFromTo 2 2 == [2] then 42 else 1\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn guarded_results_do_not_invent_a_missing_dictionary() {
    let source = "module Main where\nclass Build f where\n  build :: forall a. a -> f a\nchoose :: forall f a. Boolean -> a -> f a\nchoose = case _, _ of\n  flag, value\n    | flag -> build value\n    | true -> build value\nmain :: Int\nmain = 42\n";
    let errors = check_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
}
