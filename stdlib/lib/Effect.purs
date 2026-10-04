-- | The `Effect` monad: the corpus-facing name for the abstract effect
-- | interface this compiler already has.
-- |
-- | The operations themselves are anchored in `Prelude` rather than here, and
-- | that is deliberate rather than an oversight. Two pieces of the compiler key
-- | on that module by name: `check_run_effect_scope` resolves the trusted
-- | `Prelude.runEffect` value, and `psrs_core::effect::operations` synthesizes
-- | `pure`, `bind`, and `run` from the externals declared there. Moving the
-- | foreign imports into this module would move the entry point with them.
-- |
-- | So this module is the public surface the corpus imports, and `Prelude`
-- | remains the owner of the primitive interface. The dependency runs one way:
-- | `Effect` imports `Prelude`, never the reverse.
module Effect
  ( Effect
  , pure
  , bind
  , discard
  , map
  , apply
  , untilE
  ) where

import Prelude (Effect, apply, bind, discard, map, pure)

-- | Repeats an effect until it returns `true`.
untilE :: Effect Boolean -> Effect Unit
untilE action = bind action \done ->
  if done then pure unit else untilE action
