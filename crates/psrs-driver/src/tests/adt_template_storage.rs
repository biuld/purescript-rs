//! Shared ADT layouts retain normalized template fields across instantiations.

use super::*;
use psrs_backend::cc::{AssignmentKind, RefShape, Reference, Representation, ValueShape};

const LIBRARY: &str = r#"
module Lib where

data Payload a = Payload a (a -> a) (Array a) { value :: a }

make :: forall a. a -> Payload a
make value = Payload value (\x -> x) [value] { value: value }

raw :: forall a. Payload a -> a
raw payload = case payload of
  Payload value _ _ _ -> value

read :: forall a. Payload a -> a
read payload = case payload of
  Payload _ function values record -> function (arrayIndex values 0)

recordValue :: forall a. Payload a -> a
recordValue payload = case payload of
  Payload _ _ _ record -> record.value

data Fixed = Fixed (Int -> Int)

fixed :: Int -> Fixed
fixed ignored = Fixed (\x -> x)

applyFixed :: Fixed -> Int -> Int
applyFixed value argument = case value of
  Fixed function -> function argument
"#;

const CONSUMER: &str = r#"
module Main where
import Lib

main :: Int
main = if raw (make true) then
  if recordValue (make true) then
    case make 42 of
      Payload _ function values record -> applyFixed (fixed 0) (function (arrayIndex values 0))
  else 1
else 2
"#;

fn program() -> Vec<(&'static str, &'static str)> {
    vec![("Lib.purs", LIBRARY), ("Main.purs", CONSUMER)]
}

#[test]
fn adt_fields_retain_one_canonical_template_layout_across_instantiations() {
    let core = lower_program_to_core(&program()).expect("linked source lowers to Core");
    let cc = psrs_backend::cc::lower_module(core)
        .expect("the unspecialized program lowers to CC")
        .cc;
    let payloads = cc
        .representations
        .representations
        .iter()
        .filter_map(|repr| match repr {
            Representation::Variant { cases } if cases.len() == 1 && cases[0].fields.len() == 4 => {
                Some(&cases[0].fields)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        payloads.len(),
        1,
        "Int and Boolean share the declared variant layout"
    );
    let fields = payloads[0];
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    assert_eq!(
        fields[0], erased,
        "only the bare type-variable field uses the erased slot"
    );
    let ValueShape::Reference(Reference {
        heap: RefShape::Closure(signature),
        ..
    }) = fields[1]
    else {
        panic!("the function field must retain its template signature");
    };
    let signature = cc.representations.signature(signature).unwrap();
    assert_eq!(signature.parameters, vec![erased]);
    assert_eq!(signature.result, erased);
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(array),
        ..
    }) = fields[2]
    else {
        panic!("the array field must retain its canonical array reference");
    };
    assert_eq!(
        cc.representations.representation(array),
        Some(&Representation::Array { element: erased })
    );
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(record),
        ..
    }) = fields[3]
    else {
        panic!("the record field must retain its canonical product reference");
    };
    assert_eq!(
        cc.representations.representation(record),
        Some(&Representation::Product {
            fields: vec![erased]
        })
    );
    for function in cc
        .functions
        .iter()
        .filter(|function| ["make", "read", "recordValue"].contains(&function.name.as_str()))
    {
        assert!(
            !function.assignments.iter().any(|assignment| matches!(
                assignment.kind,
                AssignmentKind::AggregateConvert { .. }
            )),
            "generic template construction/projection needs no storage conversion: {}",
            function.name
        );
    }
}

#[test]
fn linked_template_fields_and_bare_variables_execute_at_multiple_instantiations() {
    let Some(output) = run_program_with_wasmtime(&program()) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
