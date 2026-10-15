//! Shared native computation for the Effect/ST design: an answer-indexed
//! request functor inside a phantom-result free monad, interpreted by leaf
//! storage operations and one host operation. This remains a prototype outside
//! the package; it is not public Effect/ST migration.

const NAIVE_NOMINAL_EFFECT: &str = r#"module Main where
data Native f a = Return a | Suspend (f (Native f a))
data NativeF next = NativeF next
newtype Effect a = Effect (Native NativeF a)
type role Effect representational
main :: Int
main = 0
"#;

const PHANTOM_FREE_ROLES: &str = r#"module Main where
data Val
data View f = Ret Val | Bound (f Val) (Val -> Native f Val)
data Native f a = Native (View f)
data NativeF next = NativeF next
data Global
newtype Effect a = Effect (Native NativeF a)
type role Effect representational
newtype ST r a = ST (Native NativeF a)
type role ST nominal representational
main :: Int
main = 0
"#;

const NATIVE: &str = r###"module PSRS.Effect.Native
  ( Native
  , NativeF
  , Cell
  , Effect(..)
  , ST(..)
  , Global
  , newCell
  , readCell
  , writeCell
  , clockNow
  , abort
  , runNative
  , runEffect
  , runST
  , toEffect
  ) where

import Prelude

import PSRS.Effect.Native.Host (clockNow) as Host
import Unsafe.Coerce (unsafeCoerce)

foreign import "psrs:intrinsic#arrayFill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:intrinsic#arrayWrite" slot :: forall a. Array a -> Int -> a -> Array a
foreign import "psrs:intrinsic#arrayIndex" at :: forall a. Array a -> Int -> a
foreign import "psrs:runtime-storage#trap" trap :: forall a. a

newtype Cell a = Cell (Array a)

-- Upstream Free erases the result to `Val`, so `a` is phantom and Effect/ST
-- may declare the official representational result role. `next` on NativeF is
-- the operation answer, not the rest of the program.
data Val
data View f = Ret Val | Bound (f Val) (Val -> Native f Val)
data Native f a = Native (View f)

liftF :: forall f a. Functor f => f a -> Native f a
liftF fa = Native (Bound (unsafeCoerce fa :: f Val) (\v -> Native (Ret v)))

pureNative :: forall f a. a -> Native f a
pureNative a = Native (Ret (unsafeCoerce a))

bindNative :: forall f a b. Native f a -> (a -> Native f b) -> Native f b
bindNative (Native (Ret v)) k = k (unsafeCoerce v)
bindNative (Native (Bound req cont)) k =
  Native (Bound req (\v -> bindNative (cont v) (unsafeCoerce k)))

instance functorNative :: Functor (Native f) where
  map f n = bindNative n \a -> pureNative (f a)

instance bindNativeI :: Bind (Native f) where
  bind = bindNative

instance applyNative :: Apply (Native f) where
  apply = ap

instance applicativeNative :: Applicative (Native f) where
  pure = pureNative

instance monadNative :: Monad (Native f)

type Methods next z =
  { new :: forall a. a -> (Cell a -> next) -> z
  , read :: forall a. Cell a -> (a -> next) -> z
  , write :: forall a. Cell a -> a -> (Unit -> next) -> z
  , clockNow :: (Int -> next) -> z
  , abort :: Unit -> z
  }

newtype NativeF next = NativeF (forall z. Methods next z -> z)

instance functorNativeF :: Functor NativeF where
  map g (NativeF k) = NativeF \h ->
    k
      { new: \value cont -> h.new value (cont >>> g)
      , read: \cell cont -> h.read cell (cont >>> g)
      , write: \cell value cont -> h.write cell value (cont >>> g)
      , clockNow: \cont -> h.clockNow (cont >>> g)
      , abort: h.abort
      }

newCell :: forall a. a -> Native NativeF (Cell a)
newCell value = liftF (NativeF \h -> h.new value identity)

readCell :: forall a. Cell a -> Native NativeF a
readCell cell = liftF (NativeF \h -> h.read cell identity)

writeCell :: forall a. Cell a -> a -> Native NativeF Unit
writeCell cell value = liftF (NativeF \h -> h.write cell value identity)

clockNow :: Native NativeF Int
clockNow = liftF (NativeF \h -> h.clockNow identity)

abort :: forall a. Native NativeF a
abort = liftF (NativeF \h -> h.abort unit)

runNative :: forall a. Native NativeF a -> a
runNative = go
  where
  -- Answer-indexed NativeF already stored the payload; Free holds the
  -- continuation. Do not map the continuation back into NativeF: that would
  -- compose through the erased `Val` answer and is not required to interpret.
  go :: Native NativeF a -> a
  go (Native (Ret v)) = unsafeCoerce v
  go (Native (Bound req k)) = case unsafeCoerce req :: NativeF Val of
    NativeF methods ->
      let cont :: Val -> Native NativeF a
          cont v = unsafeCoerce (k v)
      in methods
        { new: \value _ -> go (cont (unsafeCoerce (Cell (filled 1 value))))
        , read: \(Cell cell) _ -> go (cont (unsafeCoerce (at cell 0)))
        , write: \(Cell cell) value _ ->
            go (cont (unsafeCoerce (let _ = slot cell 0 value in unit)))
        , clockNow: \_ -> go (cont (unsafeCoerce Host.clockNow))
        , abort: \_ -> trap
        }

newtype Effect a = Effect (Native NativeF a)
type role Effect representational

data Global

newtype ST r a = ST (Native NativeF a)
type role ST nominal representational

toEffect :: forall a. ST Global a -> Effect a
toEffect = unsafeCoerce

runEffect :: forall a. Effect a -> a
runEffect (Effect n) = runNative n

runST :: forall a. (forall r. ST r a) -> a
runST action = runNative (unST action)
  where
  unST :: forall r. ST r a -> Native NativeF a
  unST (ST n) = n
"###;

const HOST: &str = r#"module PSRS.Effect.Native.Host (clockNow) where

foreign import "wasi:clocks/monotonic-clock#now" clockNow :: Int
"#;

const ENTRY: &str = r#"module Main where

import Prelude

import PSRS.Effect.Native
  ( Global
  , Native
  , NativeF
  , ST(..)
  , clockNow
  , newCell
  , readCell
  , runEffect
  , runST
  , toEffect
  , writeCell
  )

program :: Native NativeF Int
program = do
  ints <- newCell 0
  strs <- newCell "a"
  _ <- writeCell ints 41
  _ <- writeCell strs "b"
  stamp <- clockNow
  first <- readCell ints
  second <- readCell strs
  if stamp >= 0 && second == "b" then pure (first + 1) else pure 0

storage :: forall r. ST r Int
storage = ST program

globalStorage :: ST Global Int
globalStorage = ST program

main :: Int
main =
  let viaST = runST storage
      viaEffect = runEffect (toEffect globalStorage)
  in if viaST == 42 && viaEffect == 42 then 42 else 0
"#;

fn rejects_role(source: &str) {
    let errors = crate::check_program(&[("Main.purs", source)]).expect_err("should be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("RoleMismatch")),
        "expected RoleMismatch, got {errors:?}"
    );
}

#[test]
fn a_naive_free_monad_rejects_a_representational_effect_role() {
    rejects_role(NAIVE_NOMINAL_EFFECT);
}

#[test]
fn a_phantom_free_native_program_accepts_official_effect_and_st_roles() {
    crate::check_program(&[("Main.purs", PHANTOM_FREE_ROLES)]).unwrap_or_else(|errors| {
        panic!("phantom Free should accept official Effect/ST roles: {errors:?}")
    });
}

#[test]
fn st_global_to_effect_runs_heterogeneous_storage_and_host() {
    let Some(output) = super::run_program_with_prelude_wasmtime(&[
        ("PSRS/Effect/Native/Host.purs", HOST),
        ("PSRS/Effect/Native.purs", NATIVE),
        ("Main.purs", ENTRY),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
