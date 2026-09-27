module WASI.Poll (poll, ready, block) where

import Prelude
import WASI.Streams (Pollable)

foreign import "wasi:io/poll#poll" pollRaw :: Array Pollable -> Array Int
foreign import "wasi:io/poll#[method]pollable.ready" readyRaw :: Pollable -> Boolean
foreign import "wasi:io/poll#[method]pollable.block" blockRaw :: Pollable -> Unit

poll :: Array Pollable -> Effect (Array Int)
poll pollables = \token -> pollRaw pollables

ready :: Pollable -> Effect Boolean
ready pollable = \token -> readyRaw pollable

block :: Pollable -> Effect Unit
block pollable = \token -> blockRaw pollable
