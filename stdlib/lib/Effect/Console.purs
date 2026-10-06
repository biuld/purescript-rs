module Effect.Console where

import Effect (Effect)
import WASI.Console as Console

import Data.Show (class Show, show)
import Data.Unit (Unit)

-- | Write a message to the console.
-- WASI routes log to its log stream operation.
log :: String -> Effect Unit
log = Console.log

-- | Write a value to the console, using its `Show` instance to produce a
-- | `String`.
logShow :: forall a. Show a => a -> Effect Unit
logShow a = log (show a)

-- | Write an warning to the console.
-- WASI routes warn to its warn stream operation.
warn :: String -> Effect Unit
warn = Console.warn

-- | Write an warning value to the console, using its `Show` instance to produce
-- | a `String`.
warnShow :: forall a. Show a => a -> Effect Unit
warnShow a = warn (show a)

-- | Write an error to the console.
-- WASI routes error to its error stream operation.
error :: String -> Effect Unit
error = Console.error

-- | Write an error value to the console, using its `Show` instance to produce a
-- | `String`.
errorShow :: forall a. Show a => a -> Effect Unit
errorShow a = error (show a)

-- | Write an info message to the console.
-- WASI routes info to its log stream operation.
info :: String -> Effect Unit
info = Console.log

-- | Write an info value to the console, using its `Show` instance to produce a
-- | `String`.
infoShow :: forall a. Show a => a -> Effect Unit
infoShow a = info (show a)

-- | Write an debug message to the console.
-- WASI routes debug to its log stream operation.
debug :: String -> Effect Unit
debug = Console.log

-- | Write an debug value to the console, using its `Show` instance to produce a
-- | `String`.
debugShow :: forall a. Show a => a -> Effect Unit
debugShow a = debug (show a)

-- | Start a named timer.
foreign import time :: String -> Effect Unit

-- | Print the time since a named timer started in milliseconds.
foreign import timeLog :: String -> Effect Unit

-- | Stop a named timer and print time since it started in milliseconds.
foreign import timeEnd :: String -> Effect Unit

-- | Clears the console
foreign import clear :: Effect Unit
