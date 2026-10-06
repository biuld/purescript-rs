//! Constraints inferred inside a local binding stay with that binding when the
//! enclosing declaration has a signature. The use instantiates them, so a
//! monad that is still unknown while the binding is checked can be `Maybe` at
//! the call.

fn assert_checks(source: &str) {
    crate::check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("program should type check: {errors:?}"));
}

#[test]
fn a_signed_function_generalizes_constraints_of_its_where_binding() {
    let source = r#"
module Main where

class Bind m where
  bind :: forall a b. m a -> (a -> m b) -> m b

class Enum a where
  succ :: a -> Maybe a

data Maybe a = Nothing | Just a

instance bindMaybe :: Bind Maybe where
  bind (Just value) continuation = continuation value
  bind Nothing _ = Nothing

instance enumInt :: Enum Int where
  succ n = Just n

enumFromTo :: forall a. Enum a => a -> Maybe a
enumFromTo from = go succ from
  where
    go step value = bind (step value) (\next -> Just next)

main :: Int
main = 0
"#;
    assert_checks(source);
}

#[test]
fn a_constraint_on_an_outer_unknown_stays_with_the_enclosing_binding() {
    let source = r#"
module Main where

class Show a where
  show :: a -> Int

instance showInt :: Show Int where
  show _ = 1

shown value = let displayed = show value in displayed

main :: Int
main = shown 1
"#;
    assert_checks(source);
}

#[test]
fn a_recursive_local_keeps_a_shared_constraint_for_the_enclosing_use() {
    let source = r#"
module Main where

class Semiring a where
  add :: a -> a -> a

instance semiringInt :: Semiring Int where
  add value _ = value

loop :: Int -> Int
loop n =
  let
    go value = add value (go value)
  in
    go n

main :: Int
main = loop 1
"#;
    assert_checks(source);
}

#[test]
fn a_concrete_missing_instance_inside_a_let_is_still_rejected() {
    let source = r#"
module Main where

class Need a where
  need :: a -> a

bad :: Int -> Int
bad value = let needed = need value in needed

main :: Int
main = bad 1
"#;
    let errors =
        crate::check_program(&[("Main.purs", source)]).expect_err("Need Int has no instance");
    assert!(
        errors.iter().any(|error| {
            error.diagnostic.code == Some("NoInstanceFound")
                && error.diagnostic.message.contains("Need Int")
        }),
        "expected a missing Need Int instance, got {errors:?}"
    );
}
