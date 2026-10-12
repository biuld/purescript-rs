//! The shared native computation representation for the Effect/ST design: a
//! free computation over a rank-N heterogeneous request functor, interpreted by
//! leaf storage operations and one host operation. This is a representation
//! prototype, not the public Effect/ST facade.

#[test]
fn a_rank_n_heterogeneous_storage_and_host_program_runs() {
    let native = r###"-- Prototype of the native computation representation for the Effect/ST design.
-- It is deliberately outside the package: the design says to validate the
-- shared representation and typed requests before migrating public modules.
-- The host family is one raw leaf; the public Effect/ST facade is not here yet.
module PSRS.Effect.Native
  ( Native
  , NativeF
  , Cell
  , newCell
  , readCell
  , writeCell
  , clockNow
  , abort
  , bindNative
  , pureNative
  , runNative
  ) where

import Prelude

import PSRS.Effect.Native.Host (clockNow) as Host

-- Target storage leaves stay below the Canonical ABI, as the design requires.
foreign import "psrs:intrinsic#arrayFill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:intrinsic#arrayWrite" slot :: forall a. Array a -> Int -> a -> Array a
foreign import "psrs:intrinsic#arrayIndex" at :: forall a. Array a -> Int -> a
foreign import "psrs:runtime-storage#trap" trap :: forall a. a

newtype Cell a = Cell (Array a)

-- A rank-N request eliminator. `next` is the continuation's result, so a
-- request preserves the relationship between the cell's element type, the
-- operand, and the continuation result without a GADT.
type StorageMethods next z =
  { new :: forall a. a -> (Cell a -> next) -> z
  , read :: forall a. Cell a -> (a -> next) -> z
  , write :: forall a. Cell a -> a -> (Unit -> next) -> z
  }

newtype StorageF next = StorageF (forall z. StorageMethods next z -> z)

-- The host family. Each operation carries its checked request/result type.
type HostMethods next z =
  { clockNow :: (Int -> next) -> z
  }

newtype HostF next = HostF (forall z. HostMethods next z -> z)

data NativeF next = Storage (StorageF next) | Host (HostF next) | Abort

-- A free computation over the request functor. `Suspend` stores the request
-- with its continuation embedded, and the interpreter drives it in constant
-- stack by tail-calling the continuation.
data Native f a = Return a | Suspend (f (Native f a))

mapNative :: forall f a b. Functor f => (a -> b) -> Native f a -> Native f b
mapNative g (Return a) = Return (g a)
mapNative g (Suspend op) = Suspend (map (mapNative g) op)

bindNative :: forall f a b. Functor f => Native f a -> (a -> Native f b) -> Native f b
bindNative (Return a) f = f a
bindNative (Suspend op) f = Suspend (map (\rest -> bindNative rest f) op)

pureNative :: forall f a. a -> Native f a
pureNative = Return

instance functorNativeF :: Functor NativeF where
  map g (Storage (StorageF k)) = Storage (StorageF \h ->
    k
      { new: \value cont -> h.new value (cont >>> g)
      , read: \cell cont -> h.read cell (cont >>> g)
      , write: \cell value cont -> h.write cell value (cont >>> g)
      })
  map g (Host (HostF k)) = Host (HostF \h -> k { clockNow: \cont -> h.clockNow (cont >>> g) })
  map _ Abort = Abort

instance functorNative :: Functor f => Functor (Native f) where
  map g (Return a) = Return (g a)
  map g (Suspend op) = Suspend (map (map g) op)

instance bindNative' :: Functor f => Bind (Native f) where
  bind (Return a) f = f a
  bind (Suspend op) f = Suspend (map (\rest -> bind rest f) op)

instance applyNative :: Functor f => Apply (Native f) where
  apply mf mx = bind mf \f -> bind mx \x -> Return (f x)

instance applicativeNative :: Functor f => Applicative (Native f) where
  pure = Return

instance monadNative :: Functor f => Monad (Native f)

newCell :: forall a. a -> Native NativeF (Cell a)
newCell value = Suspend (Storage (StorageF request))
  where
  request :: forall z. StorageMethods (Native NativeF (Cell a)) z -> z
  request h = h.new value Return

readCell :: forall a. Cell a -> Native NativeF a
readCell cell = Suspend (Storage (StorageF request))
  where
  request :: forall z. StorageMethods (Native NativeF a) z -> z
  request h = h.read cell Return

writeCell :: forall a. Cell a -> a -> Native NativeF Unit
writeCell cell value = Suspend (Storage (StorageF request))
  where
  request :: forall z. StorageMethods (Native NativeF Unit) z -> z
  request h = h.write cell value Return

clockNow :: Native NativeF Int
clockNow = Suspend (Host (HostF request))
  where
  request :: forall z. HostMethods (Native NativeF Int) z -> z
  request h = h.clockNow Return

abort :: forall a. Native NativeF a
abort = Suspend Abort

runNative :: forall a. Native NativeF a -> a
runNative = go
  where
  go :: Native NativeF a -> a
  go (Return a) = a
  go (Suspend op) = go (step op)

  step :: NativeF (Native NativeF a) -> Native NativeF a
  step (Storage (StorageF k)) = k (storageInterpreter :: StorageMethods (Native NativeF a) (Native NativeF a))
  step (Host (HostF k)) = k { clockNow: \cont -> cont Host.clockNow }
  step Abort = trap

  storageInterpreter :: StorageMethods (Native NativeF a) (Native NativeF a)
  storageInterpreter =
    { new: \value cont -> cont (Cell (filled 1 value))
    , read: \(Cell cell) cont -> cont (at cell 0)
    , write: \(Cell cell) value cont -> cont (let _ = slot cell 0 value in unit)
    }
"###;
    let host = r###"module PSRS.Effect.Native.Host (clockNow) where

-- A raw host leaf: declared at its canonical result, not wrapped in `Effect`.
-- The native interpreter turns this into a Host request.
foreign import "wasi:clocks/monotonic-clock#now" clockNow :: Int
"###;
    let entry = r###"module Main where

import Prelude

import PSRS.Effect.Native (Native, NativeF, newCell, readCell, writeCell, clockNow, runNative)

-- A storage program with two element types plus a host request. The host leaf
-- is a real WASI monotonic-clock read performed by the interpreter.
program :: Native NativeF Int
program = do
  ints <- newCell 0
  strs <- newCell "a"
  _ <- writeCell ints 41
  _ <- writeCell strs "b"
  stamp <- clockNow
  first <- readCell ints
  _ <- readCell strs
  if stamp >= 0 then pure (first + 1) else pure 0

main :: Int
main = runNative program
"###;
    let Some(output) = super::run_program_with_prelude_wasmtime(&[
        ("PSRS/Effect/Native/Host.purs", host),
        ("PSRS/Effect/Native.purs", native),
        ("Main.purs", entry),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
