module Golden where
import PSRS.Array as Target.Array
arrayApply :: forall a b. Array (a -> b) -> Array a -> Array b
arrayApply = Target.Array.arrayApply
