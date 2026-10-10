//! Value-sensitive evidence for abstract constructor transport.
//!
//! The Reader program does not use Effect. It checks that a dictionary method
//! and its caller adapt a stored function protocol instead of casting one
//! closure signature onto another.

use super::*;

#[test]
fn reader_dictionary_returns_the_concrete_result() {
    let source = r#"
module Main where

class Chain f where
  chain :: forall a b. f a -> (a -> f b) -> f b

instance chainReader :: Chain ((->) Int) where
  chain m k x = k (m x) x

repeatAction :: forall f. Chain f => f Int -> f Int
repeatAction action = chain action (\_ -> action)

action :: Int -> Int
action x = x

main :: Int
main = repeatAction action 42
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"", "{output:?}");
}

/// A fixed source payload (`f Unit`) still crosses the generic method's
/// definition ABI. The method returns an `f Unit` whose stored calling
/// convention is its own, so running it executes the action twice without a
/// cast from one closure signature onto another.
#[test]
fn fixed_unit_payload_through_a_bind_constraint_repeats_the_action() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

again :: forall f. Bind f => f Unit -> f Unit
again action = bind action (\_ -> action)

main :: Effect Unit
main = again (log "again")
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"again\nagain\n", "{output:?}");
}

/// The abstract callable boundary recovers the producer's stored protocol and
/// then adapts it, rather than casting the erased closure directly onto the
/// consumer's concrete signature. The generated adapter returns the concrete
/// payload and its factory captures the producer protocol closure.
#[test]
fn abstract_callable_transport_emits_a_checked_adapter() {
    let source = r#"
module Main where

class Chain f where
  chain :: forall a b. f a -> (a -> f b) -> f b

instance chainReader :: Chain ((->) Int) where
  chain m k x = k (m x) x

repeatAction :: forall f. Chain f => f Int -> f Int
repeatAction action = chain action (\_ -> action)

action :: Int -> Int
action x = x

main :: Int
main = repeatAction action 42
"#;
    let prepared = crate::prepare_main(source).expect("source should lower to Core");
    let cc = psrs_backend::lower_cc_with_context(
        prepared.core.clone(),
        prepared.effect_context.as_ref(),
    )
    .expect("the Reader dictionary should lower to CC")
    .cc;
    let has_adapter = cc
        .functions
        .iter()
        .flat_map(|function| &function.assignments)
        .any(|assignment| {
            matches!(
                &assignment.kind,
                psrs_backend::cc::AssignmentKind::AggregateConvert { conversion, .. }
                    if plan_has_adapter(&conversion.plan)
            )
        });
    assert!(
        has_adapter,
        "the abstract callable boundary must emit a checked adapter"
    );
    assert!(
        cc.functions
            .iter()
            .any(|function| function.name.starts_with("protocol_adapter_factory_")),
        "the adapter factory must exist and capture the producer protocol closure"
    );
    assert!(
        cc.functions
            .iter()
            .filter(|function| function.name.starts_with("protocol_adapter_"))
            .any(|function| function.result_type == psrs_backend::cc::ValueShape::Integer),
        "the generated adapter must return the concrete payload, not the erased result"
    );
}

fn plan_has_adapter(plan: &psrs_backend::cc::ValueConversion) -> bool {
    use psrs_backend::cc::ValueConversion;
    match plan {
        ValueConversion::FunctionAdapter { .. } => true,
        ValueConversion::Sequence(steps) => steps.iter().any(plan_has_adapter),
        ValueConversion::ArrayMap { element, .. } => plan_has_adapter(element),
        ValueConversion::ProductMap { fields, .. } => fields.iter().any(plan_has_adapter),
        _ => false,
    }
}

#[test]
fn erased_function_slots_preserve_partial_application_and_captures() {
    let source = r#"
module Main where

data Box a = Box a

store :: forall a. a -> Box a
store value = Box value

load :: forall a. Box a -> a
load (Box value) = value

add :: Int -> Int -> Int
add x y = intAdd x y

capture :: Int -> Int -> Int -> Int
capture offset x y = intAdd offset (intAdd x y)

main :: Int
main = if intEq (load (store add) 40 2) 42
  then load (store (capture 7)) 20 15
  else 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
