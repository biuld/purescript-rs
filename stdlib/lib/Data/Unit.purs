-- | `Unit` is a compiler builtin, the same type as an unqualified
-- | `Unit`. This module re-exports that builtin and the `unit`
-- | primitive so `import Data.Unit` matches the official library
-- | without declaring a second unit type.
module Data.Unit (Unit, unit) where
