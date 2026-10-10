module Golden where
import PSRS.Array as Target.Array
arrayBind :: forall a b. Array a -> (a -> Array b) -> Array b
arrayBind = Target.Array.arrayBind
