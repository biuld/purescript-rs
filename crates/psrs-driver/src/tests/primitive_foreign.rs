use super::*;

#[test]
fn primitive_foreign_bindings_validate_unused_operand_and_result_types() {
    for ty in [
        "Int -> Int",
        "Number -> Number",
        "Int -> Number -> Number",
        "forall a. a -> a",
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#intToNumber\" convert :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("invalid primitive contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking"),
            "{errors:?}"
        );
    }
}

#[test]
fn primitive_foreign_binding_names_are_registry_operations() {
    let source = "module Main where\nforeign import \"psrs:intrinsic#missing\" convert :: Int -> Number\nmain = 0\n";
    let errors = check_program(&[("Main.purs", source)]).expect_err("unknown operation");
    assert!(
        errors.iter().any(|error| error
            .diagnostic
            .message
            .contains("unknown primitive binding")),
        "{errors:?}"
    );
}

#[test]
fn primitive_foreign_bindings_keep_unimplemented_categories_explicit() {
    let source = "module Main where\nforeign import \"psrs:intrinsic#arrayLength\" size :: forall a. Array a -> Int\nmain = 0\n";
    let errors = compile_program_sources(&[("Main.purs", source)])
        .expect_err("array foreign lowering is not implemented yet");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P8 primitive linking"
                && error
                    .diagnostic
                    .message
                    .contains("no foreign-function implementation yet")),
        "{errors:?}"
    );
}

#[test]
fn primitive_foreign_bindings_execute_as_first_class_functions() {
    let source = "module Main where\nforeign import \"psrs:intrinsic#intToNumber\" convert :: Int -> Number\nforeign import \"psrs:intrinsic#numberAdd\" add :: Number -> Number -> Number\nforeign import \"psrs:intrinsic#numberEq\" equal :: Number -> Number -> Boolean\napply f x = f x\nmain = if equal (apply (add (convert 40)) 2.0) 42.0 then 42 else 1\n";
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_foreign_bindings_preserve_cross_module_operator_identity() {
    let native = "module Native where\nforeign import \"psrs:intrinsic#intAdd\" sum :: Int -> Int -> Int\ninfixl 6 sum as %%\n";
    let main = "module Main where\nimport Native\nmain = 40 %% 2\n";
    let Some(output) = run_program_with_wasmtime(&[("Native.purs", native), ("Main.purs", main)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn vendored_bit_operations_execute_the_declared_target_contract() {
    let source = "module Main where\nimport Data.Int.Bits as Bits\nmain = if booleanAnd (intEq (Bits.and 63 42) 42) (booleanAnd (intEq (Bits.or 32 10) 42) (booleanAnd (intEq (Bits.xor 63 21) 42) (booleanAnd (intEq (Bits.shl 21 33) 42) (booleanAnd (intEq (Bits.shr (intNeg 84) 33) (intNeg 42)) (booleanAnd (intEq (Bits.zshr (intNeg 1) 1) 2147483647) (booleanAnd (intEq (Bits.zshr (intNeg 1) 0) (intNeg 1)) (intEq (Bits.complement (intNeg 43)) 42))))))) then 42 else 1\n";
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_foreign_bindings_match_pinned_official_scalar_observations() {
    let sources = [
        (
            "Golden.purs",
            include_str!("../../tests/fixtures/stdlib-scalar/Golden.purs"),
        ),
        (
            "Main.purs",
            include_str!("../../tests/fixtures/stdlib-scalar/Main.purs"),
        ),
    ];
    let modules = crate::prelude::sources().unwrap();
    for declaration in sources[0].1.lines().skip(1) {
        assert!(
            modules
                .iter()
                .any(|module| module.text.lines().any(|line| line == declaration)),
            "oracle fixture must retain the actual vendored binding: {declaration}"
        );
    }
    let Some(output) = run_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
