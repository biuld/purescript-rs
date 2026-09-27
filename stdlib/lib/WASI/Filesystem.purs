-- | Idiomatic wrappers over `wasi:filesystem`. Raw imports stay module
-- | private; the public API is expressed with library types (`Descriptor`,
-- | `Maybe`, `Either`, closed records, and an `error-code` ADT).
module WASI.Filesystem
  ( Descriptor
  , FileError(..)
  , DescriptorType(..)
  , Advice(..)
  , preopens
  , preopen
  , openAt
  , openRead
  , openWrite
  , openAppend
  , read
  , write
  , readViaStream
  , writeViaStream
  , appendViaStream
  , writeString
  , readString
  , metadataHash
  , metadataHashAt
  , getFlags
  , getType
  , readlinkAt
  , isSameObject
  , createDirectoryAt
  , removeDirectoryAt
  , unlinkFileAt
  , renameAt
  , symlinkAt
  , sync
  , syncData
  , setSize
  , advise
  , linkAt
  , filesystemErrorCode
  , dropDescriptor
  , withDescriptor
  ) where

import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import WASI.Streams (Error, InputStream, OutputStream, StreamError(..), blockingRead, blockingWriteAndFlush, dropError, dropInputStream, dropOutputStream)

-- | An owned filesystem descriptor. Drop it with `dropDescriptor`.
foreign import data Descriptor :: Type

-- | `error-code` returned by filesystem operations, mapped from the WIT enum.
data FileError
  = Access
  | WouldBlock
  | Already
  | BadDescriptor
  | Busy
  | Deadlock
  | Quota
  | Exist
  | FileTooLarge
  | IllegalByteSequence
  | InProgress
  | Interrupted
  | Invalid
  | Io
  | IsDirectory
  | Loop
  | TooManyLinks
  | MessageSize
  | NameTooLong
  | NoDevice
  | NoEntry
  | NoLock
  | InsufficientMemory
  | InsufficientSpace
  | NotDirectory
  | NotEmpty
  | NotRecoverable
  | Unsupported
  | NoTty
  | NoSuchDevice
  | Overflow
  | NotPermitted
  | Pipe
  | ReadOnly
  | InvalidSeek
  | TextFileBusy
  | CrossDevice

-- | `descriptor-type`, mapped from the WIT enum.
data DescriptorType
  = Unknown
  | BlockDevice
  | CharacterDevice
  | Directory
  | Fifo
  | SymbolicLink
  | RegularFile
  | Socket

-- | `advice`, mapped from the WIT enum.
data Advice = Normal | Sequential | Random | WillNeed | DontNeed | NoReuse

defaultPathFlags :: { symlinkFollow :: Boolean }
defaultPathFlags = { symlinkFollow: true }

defaultOpenFlags :: { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean }
defaultOpenFlags = { create: false, directory: false, exclusive: false, truncate: false }

defaultDescriptorFlags :: { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean }
defaultDescriptorFlags = { read: false, write: false, fileIntegritySync: false, dataIntegritySync: false, requestedWriteSync: false, mutateDirectory: false }

foreign import "wasi:filesystem/preopens#get-directories" preopensRaw :: Array { _1 :: Descriptor, _2 :: String }

-- | The preopened directories. The canonical ABI fixes a tuple's field names
-- | to `_1`/`_2`, so each element carries the descriptor first and its path
-- | second; `preopen` returns the friendlier named record for one entry.
preopens :: Effect (Array { _1 :: Descriptor, _2 :: String })
preopens = \token -> preopensRaw

-- | The preopened directory at `index`, as a named record.
preopen :: Int -> Effect { descriptor :: Descriptor, path :: String }
preopen index = \token ->
  let entry = arrayIndex preopensRaw index in
  { descriptor: entry._1, path: entry._2 }

foreign import "wasi:filesystem/types#[method]descriptor.open-at" openAtRaw :: Descriptor -> { symlinkFollow :: Boolean } -> String -> { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean } -> { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean } -> Either Descriptor FileError

openAt :: Descriptor -> { symlinkFollow :: Boolean } -> String -> { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean } -> { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean } -> Effect (Either FileError Descriptor)
openAt descriptor pathFlags path openFlags flags = \token ->
  case openAtRaw descriptor pathFlags path openFlags flags of
    Left opened -> Right opened
    Right err -> Left err

openRead :: Descriptor -> String -> Effect (Either FileError Descriptor)
openRead descriptor path =
  openAt descriptor defaultPathFlags path defaultOpenFlags (defaultDescriptorFlags { read = true })

openWrite :: Descriptor -> String -> Effect (Either FileError Descriptor)
openWrite descriptor path =
  openAt descriptor defaultPathFlags path (defaultOpenFlags { create = true, truncate = true }) (defaultDescriptorFlags { write = true })

openAppend :: Descriptor -> String -> Effect (Either FileError Descriptor)
openAppend descriptor path =
  openAt descriptor defaultPathFlags path defaultOpenFlags (defaultDescriptorFlags { write = true })

foreign import "wasi:filesystem/types#[method]descriptor.read" readRaw :: Descriptor -> Int -> Int -> Either { _1 :: String, _2 :: Boolean } FileError

-- | Reads up to `length` bytes at `offset`. `Right` is the decoded byte
-- | sequence, `Left` a `FileError`.
read :: Descriptor -> Int -> Int -> Effect (Either FileError String)
read descriptor length offset = \token ->
  case readRaw descriptor length offset of
    Left result -> Right (result._1)
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.write" writeRaw :: Descriptor -> String -> Int -> Either Int FileError

write :: Descriptor -> String -> Int -> Effect (Either FileError Int)
write descriptor buffer offset = \token ->
  case writeRaw descriptor buffer offset of
    Left written -> Right written
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.read-via-stream" readViaStreamRaw :: Descriptor -> Int -> Either InputStream FileError
foreign import "wasi:filesystem/types#[method]descriptor.write-via-stream" writeViaStreamRaw :: Descriptor -> Int -> Either OutputStream FileError
foreign import "wasi:filesystem/types#[method]descriptor.append-via-stream" appendViaStreamRaw :: Descriptor -> Either OutputStream FileError

readViaStream :: Descriptor -> Int -> Effect (Either FileError InputStream)
readViaStream descriptor offset = \token ->
  case readViaStreamRaw descriptor offset of
    Left stream -> Right stream
    Right err -> Left err

writeViaStream :: Descriptor -> Int -> Effect (Either FileError OutputStream)
writeViaStream descriptor offset = \token ->
  case writeViaStreamRaw descriptor offset of
    Left stream -> Right stream
    Right err -> Left err

appendViaStream :: Descriptor -> Effect (Either FileError OutputStream)
appendViaStream descriptor = \token ->
  case appendViaStreamRaw descriptor of
    Left stream -> Right stream
    Right err -> Left err

foreign import "wasi:filesystem/types#filesystem-error-code" filesystemErrorCodeRaw :: Error -> Maybe FileError

-- | Recovers a filesystem `error-code` from a stream operation failure. The
-- | `Error` handle is borrowed by the WIT call.
filesystemErrorCode :: Error -> Effect (Maybe FileError)
filesystemErrorCode err = \token -> filesystemErrorCodeRaw err

-- | Writes `contents` through an output stream. `Nothing` means the write
-- | succeeded; `Just` is the error from opening the stream. A stream-write
-- | failure traps, matching the WIT unit-success result.
writeString :: Descriptor -> String -> Effect (Maybe FileError)
writeString descriptor contents =
  bind (writeViaStream descriptor 0) (\opened ->
    case opened of
      Left err -> pure (Just err)
      Right stream ->
        bind (blockingWriteAndFlush stream contents) (\ignored ->
          bind (dropOutputStream stream) (\ignoredStream ->
            pure Nothing)))

-- | Reads up to `length` bytes through an input stream.
readString :: Descriptor -> Int -> Effect (Either FileError String)
readString descriptor length =
  bind (readViaStream descriptor 0) (\opened ->
    case opened of
      Left err -> pure (Left err)
      Right stream ->
        bind (blockingRead stream length) (\contents ->
          bind (dropInputStream stream) (\ignored ->
            case contents of
              Left bytes -> pure (Right bytes)
              Right (LastOperationFailed err) ->
                bind (filesystemErrorCode err) (\code ->
                  bind (dropError err) (\ignoredError ->
                    case code of
                      Just fileError -> pure (Left fileError)
                      Nothing -> pure (Left Io)))
              Right Closed -> pure (Left Io))))

foreign import "wasi:filesystem/types#[method]descriptor.metadata-hash" metadataHashRaw :: Descriptor -> Either { lower :: Int, upper :: Int } FileError
foreign import "wasi:filesystem/types#[method]descriptor.metadata-hash-at" metadataHashAtRaw :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Either { lower :: Int, upper :: Int } FileError

metadataHash :: Descriptor -> Effect (Either FileError { lower :: Int, upper :: Int })
metadataHash descriptor = \token ->
  case metadataHashRaw descriptor of
    Left hash -> Right hash
    Right err -> Left err

metadataHashAt :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Effect (Either FileError { lower :: Int, upper :: Int })
metadataHashAt descriptor pathFlags path = \token ->
  case metadataHashAtRaw descriptor pathFlags path of
    Left hash -> Right hash
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.get-flags" getFlagsRaw :: Descriptor -> Either { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean } FileError

getFlags :: Descriptor -> Effect (Either FileError { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean })
getFlags descriptor = \token ->
  case getFlagsRaw descriptor of
    Left flags -> Right flags
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.get-type" getTypeRaw :: Descriptor -> Either DescriptorType FileError

getType :: Descriptor -> Effect (Either FileError DescriptorType)
getType descriptor = \token ->
  case getTypeRaw descriptor of
    Left descriptorType -> Right descriptorType
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.readlink-at" readlinkAtRaw :: Descriptor -> String -> Either String FileError

readlinkAt :: Descriptor -> String -> Effect (Either FileError String)
readlinkAt descriptor path = \token ->
  case readlinkAtRaw descriptor path of
    Left target -> Right target
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.is-same-object" isSameObjectRaw :: Descriptor -> Descriptor -> Boolean

isSameObject :: Descriptor -> Descriptor -> Effect Boolean
isSameObject left right = \token -> isSameObjectRaw left right

-- The remaining operations report `result<_, error-code>`: the canonical ABI
-- maps that to a `Unit` result that traps on failure, so these wrappers cannot
-- return a `FileError`.
foreign import "wasi:filesystem/types#[method]descriptor.create-directory-at" createDirectoryAtRaw :: Descriptor -> String -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.remove-directory-at" removeDirectoryAtRaw :: Descriptor -> String -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.unlink-file-at" unlinkFileAtRaw :: Descriptor -> String -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.rename-at" renameAtRaw :: Descriptor -> String -> Descriptor -> String -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.symlink-at" symlinkAtRaw :: Descriptor -> String -> String -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.sync" syncRaw :: Descriptor -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.sync-data" syncDataRaw :: Descriptor -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.set-size" setSizeRaw :: Descriptor -> Int -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.advise" adviseRaw :: Descriptor -> Int -> Int -> Advice -> Unit
foreign import "wasi:filesystem/types#[method]descriptor.link-at" linkAtRaw :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Descriptor -> String -> Unit

createDirectoryAt :: Descriptor -> String -> Effect Unit
createDirectoryAt descriptor path = \token -> createDirectoryAtRaw descriptor path

removeDirectoryAt :: Descriptor -> String -> Effect Unit
removeDirectoryAt descriptor path = \token -> removeDirectoryAtRaw descriptor path

unlinkFileAt :: Descriptor -> String -> Effect Unit
unlinkFileAt descriptor path = \token -> unlinkFileAtRaw descriptor path

renameAt :: Descriptor -> String -> Descriptor -> String -> Effect Unit
renameAt descriptor oldPath newDescriptor newPath = \token ->
  renameAtRaw descriptor oldPath newDescriptor newPath

symlinkAt :: Descriptor -> String -> String -> Effect Unit
symlinkAt descriptor oldPath newPath = \token -> symlinkAtRaw descriptor oldPath newPath

sync :: Descriptor -> Effect Unit
sync descriptor = \token -> syncRaw descriptor

syncData :: Descriptor -> Effect Unit
syncData descriptor = \token -> syncDataRaw descriptor

setSize :: Descriptor -> Int -> Effect Unit
setSize descriptor size = \token -> setSizeRaw descriptor size

advise :: Descriptor -> Int -> Int -> Advice -> Effect Unit
advise descriptor offset length advice = \token -> adviseRaw descriptor offset length advice

linkAt :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Descriptor -> String -> Effect Unit
linkAt descriptor pathFlags oldPath newDescriptor newPath = \token ->
  linkAtRaw descriptor pathFlags oldPath newDescriptor newPath

foreign import "wasi:filesystem/types#[resource-drop]descriptor" dropDescriptorRaw :: Descriptor -> Unit

dropDescriptor :: Descriptor -> Effect Unit
dropDescriptor descriptor = \token -> dropDescriptorRaw descriptor

-- | Runs `action` with `descriptor`, then drops it. The descriptor is always
-- | released once `action` completes.
withDescriptor :: forall a. Descriptor -> (Descriptor -> Effect a) -> Effect a
withDescriptor descriptor action =
  \token ->
    let result = action descriptor token in
    let ignored = dropDescriptor descriptor token in
    result
