module Golden where
import PSRS.Array as Target.Array
arrayExtend :: forall a b. (Array a -> b) -> Array a -> Array b
arrayExtend = Target.Array.arrayExtend
