use super::*;

fn run_library_program_with_wasmtime(sources: &[(&str, &str)]) -> Option<std::process::Output> {
    let (sources, _) = crate::prelude::prepend(sources).unwrap();
    run_program_with_wasmtime(&sources)
}

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
    let source =
        "module Main where\nforeign import \"psrs:intrinsic#unit\" singleton :: Unit\nmain = 0\n";
    let errors = compile_program_sources(&[("Main.purs", source)])
        .expect_err("nullary foreign lowering is not implemented yet");
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
    let Some(output) = run_library_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_foreign_bindings_preserve_cross_module_operator_identity() {
    let native = "module Native where\nforeign import \"psrs:intrinsic#intAdd\" sum :: Int -> Int -> Int\ninfixl 6 sum as %%\n";
    let main = "module Main where\nimport Native\nmain = 40 %% 2\n";
    let Some(output) =
        run_library_program_with_wasmtime(&[("Native.purs", native), ("Main.purs", main)])
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
    let Some(output) = run_library_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_foreign_array_schemes_execute_multiple_element_instantiations() {
    let native = r#"module Native where
foreign import "psrs:intrinsic#arrayLength" size :: forall a. Array a -> Int
foreign import "psrs:intrinsic#arrayIndex" at :: forall a. Array a -> Int -> a
foreign import "psrs:intrinsic#arrayUpdate" replace :: forall a. Array a -> Int -> a -> Array a
foreign import "psrs:intrinsic#arrayAppend" append :: forall a. Array a -> Array a -> Array a
"#;
    let main = r#"module Main where
import Native
apply f x = f x
main =
  let
    original = [1, 2]
    joined = append original [40]
    changed = replace joined 0 9
    texts = append ["λ"] ["😀"]
    records = replace [{ value: 1 }] 0 { value: 42 }
  in if booleanAnd (intEq (size joined) 3)
    (booleanAnd (intEq (at original 0) 1)
    (booleanAnd (intEq (at joined 0) 1)
    (booleanAnd (intEq (at changed 0) 9)
    (booleanAnd (intEq (apply (at joined) 2) 40)
    (booleanAnd (numberEq (at [1.0, 2.0] 1) 2.0)
    (booleanAnd (intEq (size texts) 2)
    (booleanAnd (intEq (arrayLength (stringToBytes (at texts 1))) 4)
    (intEq (at records 0).value 42)))))))) then 42 else 1
"#;
    let Some(output) =
        run_library_program_with_wasmtime(&[("Native.purs", native), ("Main.purs", main)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_foreign_array_contracts_reject_inconsistent_quantified_elements() {
    for (operation, ty) in [
        ("arrayFill", "forall a b. Int -> a -> Array b"),
        ("arrayFill", "forall a. a -> a -> Array a"),
        ("arrayWrite", "forall a b. Array a -> Int -> b -> Array a"),
        ("arrayWrite", "forall a b. Array a -> Int -> a -> Array b"),
        ("arrayAppend", "forall a b. Array a -> Array b -> Array a"),
        ("arrayIndex", "forall a b. Array a -> Int -> b"),
        ("arrayUpdate", "forall a b. Array a -> Int -> b -> Array a"),
        ("arrayLength", "forall a. Array a -> a"),
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#{operation}\" f :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("wrong array binding contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking"),
            "{errors:?}"
        );
    }
}

#[test]
fn library_array_apply_preserves_callback_order_and_captures() {
    let source = r#"module Main where
import PSRS.Array (arrayApply)
cartesian = arrayApply
add offset x = intAdd offset x
main =
  let
    input = [10, 20]
    result = cartesian [add 1, \x -> intMul x 2] input
    numbers = cartesian [\x -> numberAdd (intToNumber x) 0.5] [4, 8]
    records = cartesian [\x -> { value: intAdd x 2 }] [40]
    nested = cartesian [\x -> [x, intAdd x 1]] [5, 9]
    emptyValues = cartesian [\x -> arrayIndex ([] :: Array Int) x] []
    emptyFunctions = cartesian ([] :: Array (Int -> Int)) [1, 2]
  in if booleanAnd (intEq (arrayLength result) 4)
    (booleanAnd (intEq (arrayIndex result 0) 11)
    (booleanAnd (intEq (arrayIndex result 1) 21)
    (booleanAnd (intEq (arrayIndex result 2) 20)
    (booleanAnd (intEq (arrayIndex result 3) 40)
    (booleanAnd (numberEq (arrayIndex numbers 1) 8.5)
    (booleanAnd (intEq (arrayIndex records 0).value 42)
    (booleanAnd (intEq (arrayIndex (arrayIndex nested 1) 1) 10)
    (booleanAnd (intEq (arrayLength emptyValues) 0)
    (booleanAnd (intEq (arrayLength emptyFunctions) 0)
    (intEq (arrayIndex input 0) 10)))))))))) then 42 else 1
"#;
    let Some(output) = run_library_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn library_array_apply_returns_curried_functions_through_aliases() {
    for operation in ["foreignApply", "arrayApply"] {
        let source = format!(
            r#"module Main where
import PSRS.Array (arrayApply)
foreignApply = arrayApply
add x y = intAdd x y
main = (arrayIndex ({operation} [add] [40]) 0) 2
"#
        );
        let Some(output) = run_library_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{output:?}");
    }
}

#[test]
fn array_apply_traps_before_wrapping_an_unrepresentable_result_length() {
    let source = r#"module Main where
import PSRS.Array (arrayApply)
cartesian = arrayApply
double n xs = if intEq n 0 then xs else double (intSub n 1) (arrayAppend xs xs)
main = arrayLength (cartesian (double 16 [\x -> x]) (double 16 [0]))
"#;
    let Some(output) = run_library_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert!(
        !output.status.success(),
        "the 2^32 product must trap, not return an empty array"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("out of bounds array access"),
        "{output:?}"
    );
}

#[test]
fn library_array_apply_matches_pinned_official_observations() {
    let golden = include_str!("../../tests/fixtures/stdlib-array/Golden.purs");
    let declaration = golden
        .lines()
        .find(|line| line.starts_with("arrayApply ::"))
        .unwrap();
    let module = crate::prelude::sources()
        .unwrap()
        .iter()
        .find(|module| module.module_name == "Control.Apply")
        .unwrap();
    assert!(
        module.text.lines().any(|line| line == declaration),
        "fixture must retain the actual library binding"
    );
    let sources = [
        ("Golden.purs", golden),
        (
            "Main.purs",
            include_str!("../../tests/fixtures/stdlib-array/Main.purs"),
        ),
    ];
    let Some(output) = run_library_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_array_fill_and_write_preserve_initialization_and_aliases() {
    let source = r#"module Main where
foreign import "psrs:intrinsic#arrayFill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:intrinsic#arrayWrite" write :: forall a. Array a -> Int -> a -> Array a
main =
  let
    xs = filled 3 1
    put = write xs 1
    ys = put 42
    ignored = arrayWrite xs 2 9
    texts = filled 2 "😀"
    records = filled 2 { value: 42 }
    functions = filled 2 (\x -> intAdd x 2)
    empty = filled 0 "λ"
  in if booleanAnd (intEq (arrayLength xs) 3)
    (booleanAnd (intEq (arrayIndex xs 1) 42)
    (booleanAnd (intEq (arrayIndex ys 1) 42)
    (booleanAnd (intEq (arrayIndex xs 2) 9)
    (booleanAnd (intEq (arrayLength (stringToBytes (arrayIndex texts 1))) 4)
    (booleanAnd (intEq (arrayIndex records 1).value 42)
    (booleanAnd (intEq ((arrayIndex functions 1) 40) 42)
    (intEq (arrayLength empty) 0))))))) then 42 else 1
"#;
    let Some(output) = run_library_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn primitive_array_fill_and_write_trap_on_invalid_lengths_or_indices() {
    for body in [
        "arrayLength (filled (intNeg 1) 0)",
        "arrayLength (write [1] 1 2)",
        "arrayLength (write [1] (intNeg 1) 2)",
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#arrayFill\" filled :: forall a. Int -> a -> Array a\nforeign import \"psrs:intrinsic#arrayWrite\" write :: forall a. Array a -> Int -> a -> Array a\nmain = {body}\n"
        );
        let Some(output) = run_library_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert!(!output.status.success(), "{body}: {output:?}");
    }
}

#[test]
fn library_array_bind_matches_pinned_official_observations() {
    let golden = include_str!("../../tests/fixtures/stdlib-array-bind/Golden.purs");
    let signature = golden
        .lines()
        .find(|line| line.starts_with("arrayBind ::"))
        .unwrap();
    let modules = crate::prelude::sources().unwrap();
    let native = modules
        .iter()
        .find(|module| module.module_name == "Control.Bind")
        .unwrap();
    assert!(native.text.lines().any(|line| line == signature));
    let sources = [
        ("Golden.purs", golden),
        (
            "Main.purs",
            include_str!("../../tests/fixtures/stdlib-array-bind/Main.purs"),
        ),
    ];
    let Some(output) = run_library_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn library_array_extend_matches_pinned_official_observations() {
    let golden = include_str!("../../tests/fixtures/stdlib-array-extend/Golden.purs");
    let signature = golden
        .lines()
        .find(|line| line.starts_with("arrayExtend ::"))
        .unwrap();
    let modules = crate::prelude::sources().unwrap();
    let native = modules
        .iter()
        .find(|module| module.module_name == "Control.Extend")
        .unwrap();
    assert!(native.text.lines().any(|line| line == signature));
    let sources = [
        ("Golden.purs", golden),
        (
            "Main.purs",
            include_str!("../../tests/fixtures/stdlib-array-extend/Main.purs"),
        ),
    ];
    let Some(output) = run_library_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
