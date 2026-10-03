-- | The primitive surface the rest of the library and the corpus build on.
-- |
-- | `Effect` is an abstract type here: the compiler owns its representation
-- | rather than this file, and `psrs_core::effect::lower_effects` rewrites each
-- | `Effect a` application into a closure over the runtime token after Typed
-- | Core. The operations below are the externals that lowering supplies bodies
-- | for, so they stay in this module by name — `check_run_effect_scope`
-- | resolves the trusted `Prelude.runEffect` value, and
-- | `psrs_core::effect::operations` dispatches on the `psrs:effect` bindings
-- | declared here.
-- |
-- | `unit` is not declared here either. `Unit` is a builtin type with no
-- | constructor table, so the one `Unit` value is a compiler primitive
-- | (`psrs_hir::Intrinsic::Unit`) rather than a library declaration, exactly as
-- | `true` and `false` are.
module Prelude where

foreign import data Effect :: Type -> Type

foreign import "psrs:effect#pure" pure :: forall a. a -> Effect a

foreign import "psrs:effect#bind" bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b

discard :: forall a b. Effect a -> (a -> Effect b) -> Effect b
discard first next = bind first next

map :: forall a b. (a -> b) -> Effect a -> Effect b
map f x = bind x (\v -> pure (f v))

apply :: forall a b. Effect (a -> b) -> Effect a -> Effect b
apply f x = bind f (\g -> bind x (\v -> pure (g v)))

foreign import "psrs:effect#run" runEffect :: forall a. Effect a -> a

-- | The effect that escapes instead of returning. An uncaught failure on this
-- | target is a guest trap, so this is the one operation whose result never
-- | exists; it is how a library reports an assertion that did not hold.
foreign import "psrs:effect#trap" trap :: Effect Unit
