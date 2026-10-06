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
fn a_guarded_case_use_instantiates_a_where_binding() {
    let source = r#"
module Main where

class Bind m where
  bind :: forall a b. m a -> (a -> m b) -> m b

class Enum a where
  succ :: a -> Maybe a
  pred :: a -> Maybe a

data Maybe a = Nothing | Just a

instance bindMaybe :: Bind Maybe where
  bind (Just value) continuation = continuation value
  bind Nothing _ = Nothing

instance enumInt :: Enum Int where
  succ n = Just n
  pred n = Just n

enumFromTo :: forall a. Enum a => a -> a -> Maybe a
enumFromTo = case _, _ of
  from, to
    | true -> go succ from
    | true -> go pred from
  where
    go step value = bind (step value) (\next -> Just next)

main :: Int
main = 0
"#;
    assert_checks(source);
}

#[test]
fn a_where_stepper_accepts_an_integer_seed() {
    let source = r#"
module Main where

class Semiring a where
  add :: a -> a -> a
  sub :: a -> a -> a

instance semiringInt :: Semiring Int where
  add x _ = x
  sub x _ = x

infixl 6 add as +
infixl 6 sub as -

class Ord a where
  le :: a -> a -> Boolean

instance ordInt :: Ord Int where
  le _ _ = true

infix 4 le as <=

class Ord a <= BoundedEnum a where
  toEnum :: Int -> Maybe a
  fromEnum :: a -> Int

data Maybe a = Nothing | Just a
data Tuple a b = Tuple a b

class Partial

fromJust :: forall a. Partial => Maybe a -> a
fromJust (Just value) = value

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

infixl 4 map as <$>

class Unfoldable t where
  unfoldr :: forall a b. (b -> Maybe (Tuple a b)) -> b -> t a

class Semigroupoid a where
  compose :: forall b c d. a c d -> a b c -> a b d

instance semigroupoidFn :: Semigroupoid (->) where
  compose f g value = f (g value)

composeFlipped :: forall a b c d. Semigroupoid a => a b c -> a c d -> a b d
composeFlipped f g = compose g f

infixr 9 composeFlipped as >>>

unsafePartial :: forall a. (Partial => a) -> a
unsafePartial = discharge

discharge :: forall a b. a -> b
discharge value = discharge value

otherwise = true

enumFromThenTo :: forall f a. Unfoldable f => Functor f => BoundedEnum a => a -> a -> a -> f a
enumFromThenTo = unsafePartial \a b c ->
  let
    a' = fromEnum a
    b' = fromEnum b
    c' = fromEnum c
  in
    (toEnum >>> fromJust) <$> unfoldr (go (b' - a') c') a'
  where
    go step to index
      | index <= to = Just (Tuple index (index + step))
      | otherwise = Nothing

main :: Int
main = 0
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

#[test]
fn a_local_constraint_does_not_choose_an_unrelated_lexical_given() {
    assert_checks(
        r#"
module Main where

class Measure a where
  measure :: a -> Int

instance measureInt :: Measure Int where
  measure value = value

outer :: forall a. Measure a => a -> Int
outer value = measured 1
  where
    measured input = measure input

main :: Int
main = outer 0
"#,
    );
}

#[test]
fn a_superclass_functional_dependency_improves_a_local_result() {
    assert_checks(
        r#"
module Main where

class Convert a b | a -> b where
  convert :: a -> b

class Convert a b <= Middle a b
class Middle a b <= Child a b

outer :: forall a b. Child a b => a -> Int
outer value = let converted = convert value in 0

main :: Int
main = 0
"#,
    );
}

#[test]
fn a_solved_local_dictionary_uses_the_local_schemes_quantified_variables() {
    assert_checks(
        r#"
module Main where

class Delay l where
  delay :: (Int -> l) -> l

data List a = Nil | Cons a (List a)

instance delayList :: Delay (List a) where
  delay thunk = thunk 0

class Build f where
  build :: forall a. a -> f a

instance buildList :: Build List where
  build = go
    where
      go value = delay (\_ -> Cons value Nil)

main :: Int
main = 0
"#,
    );
}

#[test]
fn an_unquantified_variable_of_a_solved_local_constraint_stays_ambiguous() {
    let source = r#"
module Main where

class Delay l where
  delay :: (Int -> l) -> l

data List a = Nil
instance delayList :: Delay (List a) where
  delay thunk = thunk 0

class Choose a where
  choose :: List a -> Int

instance chooseInt :: Choose Int where
  choose _ = 0

class Build f where
  build :: forall a. a -> f a

instance buildList :: Build List where
  build _ = let discarded = choose (delay (\_ -> Nil)) in Nil

main :: Int
main = 0
"#;
    let errors = crate::check_program(&[("Main.purs", source)])
        .expect_err("the local result does not quantify the delayed element type");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("AmbiguousTypeVariables")),
        "expected the unquantified constraint to stay ambiguous: {errors:?}"
    );
}

#[test]
fn a_signed_body_owns_its_unobservable_instantiation_variables() {
    assert_checks(
        r#"
module Main where

data Proxy a = Proxy

discard :: forall a. Proxy a -> Int
discard _ = 0

main :: Int
main = discard Proxy
"#,
    );
}

#[test]
fn a_local_body_owns_its_unobservable_instantiation_variables() {
    assert_checks(
        r#"
module Main where

data Proxy a = Proxy

discard :: forall a. Proxy a -> Int
discard _ = 0

main = let discarded = discard Proxy in 0
"#,
    );
}

#[test]
fn an_instance_body_owns_its_unobservable_instantiation_variables() {
    assert_checks(
        r#"
module Main where

data Proxy a = Proxy

discard :: forall a. Proxy a -> Int
discard _ = 0

class Measure f where
  measure :: forall a. f a -> Int

instance measureProxy :: Measure Proxy where
  measure _ = discard Proxy

main :: Int
main = 0
"#,
    );
}

#[test]
fn body_only_quantifiers_preserve_execution() {
    let source = r#"
module Main where

data Proxy a = Proxy

discard :: forall a. Proxy a -> Int
discard _ = 21

class Measure f where
  measure :: forall a. f a -> Int

instance measureProxy :: Measure Proxy where
  measure _ = discard Proxy

main :: Int
main = let part = discard Proxy in intAdd part (measure Proxy)
"#;
    let Some(output) = super::run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
