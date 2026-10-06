module Golden where
foreign import "psrs:intrinsic#arrayApply" arrayApply :: forall a b. Array (a -> b) -> Array a -> Array b
