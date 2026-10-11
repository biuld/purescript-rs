//! Checked compiler primitives used as ordinary values and partial applications.

mod control_support;
use control_support::execute;

#[test]
fn aliases_preserve_control_and_scalar_primitive_semantics() {
    execute(
        "primitive-aliases",
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
pure :: forall f a. a -> C.Control f a
pure = C.return
bind :: forall f a b. C.Control f a -> (a -> C.Control f b) -> C.Control f b
bind = C.bind
perform :: forall f a. f a -> C.Control f a
perform = C.perform
run :: forall f a s. s -> (forall x. s -> f x -> (x -> C.Control f a) -> { _1 :: s, _2 :: C.Control f a }) -> C.Control f a -> { _1 :: s, _2 :: a }
run = C.run
once :: forall f a b. (a -> C.Control f b) -> C.Resumption f a b
once = C.once
resume :: forall f a b. C.Resumption f a b -> a -> C.Control f b
resume = C.resume
add :: Int -> Int -> Int
add = intAdd
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler s (Op k) cont = { _1: s, _2: resume (once cont) (k 41) }
program :: C.Control Op Int
program = bind (perform (Op \x -> x)) (\value -> pure (add value 1))
main :: Int
main = (run 0 handler program)._2
"#,
        42,
    );
}

#[test]
fn partial_applications_keep_checked_argument_types() {
    execute(
        "primitive-partial",
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler s (Op k) cont = { _1: s, _2: cont (k 0) }
finish :: (Int -> C.Control Op Int) -> C.Control Op Int
finish = C.bind (C.return 41)
interpret :: C.Control Op Int -> { _1 :: Int, _2 :: Int }
interpret = C.run 0 handler
increment :: Int -> Int
increment = intAdd 1
main :: Int
main = (interpret (finish (\value -> C.return (increment value))))._2
"#,
        42,
    );
}
