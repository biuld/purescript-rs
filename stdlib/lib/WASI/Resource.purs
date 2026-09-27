module WASI.Resource (Resource(..), unResource, withResource) where

import Prelude

newtype Resource a = Resource Int

unResource :: forall a. Resource a -> Int
unResource (Resource index) = index

withResource :: forall a b. (Resource a -> Effect Unit) -> Resource a -> (Resource a -> Effect b) -> Effect b
withResource release resource action =
  \token ->
    let result = action resource token in
    let ignored = release resource token in
    result
