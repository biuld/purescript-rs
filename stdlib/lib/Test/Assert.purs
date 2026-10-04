-- | The corpus's assertion surface: the `Test.Assert` module the `passing`
-- | suite imports.
-- |
-- | A failed assertion must be visible to whatever runs the program, and this
-- | target's only such signal is a guest trap — a non-zero exit code is a
-- | recorded result, not a failure. So the failure path writes the message to
-- | standard error and then escapes through `Prelude.trap`
-- | ([DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md): the
-- | wrapper owns the corpus name, the primitive stays in `Prelude`).
-- |
-- | **Deliberately absent**, with the reason recorded rather than approximated:
-- |
-- | - `assertEqual` and `assertEqual'` compare with `Eq` and print with `Show`.
-- |   Both classes are declared (`Data.Eq`, `Data.Show`), so the surface itself
-- |   is writable — the official signature is what this compiler cannot yet
-- |   elaborate. The corpus calls the record form
-- |   (`assertEqual' "label" { expected: e, actual: a }`), whose official type is
-- |   `forall a. Eq a => Show a => String -> { actual :: a, expected :: a } ->
-- |   Effect Unit`. A constraint whose quantified variable appears inside a
-- |   record type is elaborated with the *record* as the constraint's argument,
-- |   so `Eq a` is wanted for `{ actual :: a, expected :: a }` and the
-- |   declaration is rejected with `NoInstanceFound`. The same signature with a
-- |   type synonym for the record fails identically, so it is constraint
-- |   elaboration rather than the record syntax. Approximating the signature
-- |   would change the official API the corpus calls, so the functions stay out
-- |   until that is fixed. #137 carries the minimal reproduction and the probes
-- |   that separate this defect from record syntax.
-- | - `assertThrows` and `assertThrows'` need to observe that evaluating an
-- |   argument failed. A trap is not observable from inside the guest without
-- |   the Wasm exceptions proposal, which is outside the target profile
-- | ([DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md)), so there is
-- |   no honest implementation to write yet.
module Test.Assert (assert, assert', assertTrue, assertFalse) where

import Prelude
import Effect (Effect)
import Effect.Console (error)

-- | Escapes when the boolean is false. The message is written to standard
-- | error first, so the trap carries the diagnostic with it.
assert' :: String -> Boolean -> Effect Unit
assert' message condition =
  if condition
  then pure unit
  else abortWith message

-- | Escapes with the default message when the boolean is false.
assert :: Boolean -> Effect Unit
assert = assert' "Assertion failed"

-- | Escapes unless the value is `true`, naming both values.
assertTrue :: Boolean -> Effect Unit
assertTrue actual =
  if actual
  then pure unit
  else abortWith "Assertion failed: Expected: true\nActual:   false"

-- | Escapes unless the value is `false`, naming both values.
assertFalse :: Boolean -> Effect Unit
assertFalse actual =
  if actual
  then abortWith "Assertion failed: Expected: false\nActual:   true"
  else pure unit

-- | The escape itself. Kept private: a caller reports a failure by choosing the
-- | message, not by reaching for the trap.
abortWith :: String -> Effect Unit
abortWith message = do
  _ <- error message
  trap
