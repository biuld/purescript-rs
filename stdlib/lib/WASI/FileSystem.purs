-- | Idiomatic wrappers over `wasi:filesystem`. Raw imports stay module
-- | private; the public API is expressed with `Resource a` handles and library
-- | types (`Maybe`, `Either`, closed records, and an `error-code` ADT). Every
-- | `result` maps to `Either` with the error on `Left`.
module WASI.FileSystem
  ( Descriptor
  , DirectoryEntryStream
  , FileError(..)
  , DescriptorType(..)
  , Advice(..)
  , NewTimestamp(..)
  , preopens
  , preopen
  , openAt
  , openRead
  , openWrite
  , openAppend
  , readFile
  , writeFile
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
  , setTimes
  , setTimesAt
  , advise
  , linkAt
  , stat
  , statAt
  , readDirectory
  , readDirectoryEntry
  , dropDirectoryEntryStream
  , filesystemErrorCode
  , dropDescriptor
  , withDescriptor
  ) where

import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import WASI.IO (Error, InputStream, OutputStream, StreamError(..), blockingRead, blockingWriteAndFlush, dropError, dropInputStream, dropOutputStream)
import WASI.Resource (Resource)

-- | The opaque phantom naming a filesystem descriptor resource.
foreign import data Descriptor :: Type

-- | The opaque phantom naming a directory-entry stream resource.
foreign import data DirectoryEntryStream :: Type

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

-- | `new-timestamp` taken by `setTimes` and `setTimesAt`: leave the timestamp
-- | unchanged, set it to now, or set it to a given `datetime`.
data NewTimestamp
  = NoChange
  | Now
  | Timestamp { seconds :: Int, nanoseconds :: Int }

defaultPathFlags :: { symlinkFollow :: Boolean }
defaultPathFlags = { symlinkFollow: true }

defaultOpenFlags :: { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean }
defaultOpenFlags = { create: false, directory: false, exclusive: false, truncate: false }

defaultDescriptorFlags :: { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean }
defaultDescriptorFlags = { read: false, write: false, fileIntegritySync: false, dataIntegritySync: false, requestedWriteSync: false, mutateDirectory: false }

foreign import "wasi:filesystem/preopens#get-directories" preopensRaw :: Array { _1 :: Resource Descriptor, _2 :: String }

-- | The preopened directories. The canonical ABI fixes a tuple's field names
-- | to `_1`/`_2`, so each element carries the descriptor first and its path
-- | second; `preopen` returns the friendlier named record for one entry.
preopens :: Effect (Array { _1 :: Resource Descriptor, _2 :: String })
preopens = \token -> preopensRaw

-- | The preopened directory at `index`, as a named record.
preopen :: Int -> Effect { descriptor :: Resource Descriptor, path :: String }
preopen index = \token ->
  let entry = arrayIndex preopensRaw index in
  { descriptor: entry._1, path: entry._2 }

foreign import "wasi:filesystem/types#[method]descriptor.open-at" openAtRaw :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean } -> { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean } -> Either FileError (Resource Descriptor)

openAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean } -> { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean } -> Effect (Either FileError (Resource Descriptor))
openAt descriptor pathFlags path openFlags flags = \token ->
  openAtRaw descriptor pathFlags path openFlags flags

openRead :: Resource Descriptor -> String -> Effect (Either FileError (Resource Descriptor))
openRead descriptor path =
  openAt descriptor defaultPathFlags path defaultOpenFlags (defaultDescriptorFlags { read = true })

openWrite :: Resource Descriptor -> String -> Effect (Either FileError (Resource Descriptor))
openWrite descriptor path =
  openAt descriptor defaultPathFlags path (defaultOpenFlags { create = true, truncate = true }) (defaultDescriptorFlags { write = true })

openAppend :: Resource Descriptor -> String -> Effect (Either FileError (Resource Descriptor))
openAppend descriptor path =
  openAt descriptor defaultPathFlags path defaultOpenFlags (defaultDescriptorFlags { write = true })

foreign import "wasi:filesystem/types#[method]descriptor.read" readRaw :: Resource Descriptor -> Int -> Int -> Either FileError { _1 :: String, _2 :: Boolean }

-- | Reads up to `length` bytes at `offset`. `Right` is the decoded byte
-- | sequence, `Left` a `FileError`.
readFile :: Resource Descriptor -> Int -> Int -> Effect (Either FileError String)
readFile descriptor length offset = \token ->
  case readRaw descriptor length offset of
    Right result -> Right (result._1)
    Left err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.write" writeRaw :: Resource Descriptor -> String -> Int -> Either FileError Int

writeFile :: Resource Descriptor -> String -> Int -> Effect (Either FileError Int)
writeFile descriptor buffer offset = \token ->
  writeRaw descriptor buffer offset

foreign import "wasi:filesystem/types#[method]descriptor.read-via-stream" readViaStreamRaw :: Resource Descriptor -> Int -> Either FileError (Resource InputStream)
foreign import "wasi:filesystem/types#[method]descriptor.write-via-stream" writeViaStreamRaw :: Resource Descriptor -> Int -> Either FileError (Resource OutputStream)
foreign import "wasi:filesystem/types#[method]descriptor.append-via-stream" appendViaStreamRaw :: Resource Descriptor -> Either FileError (Resource OutputStream)

readViaStream :: Resource Descriptor -> Int -> Effect (Either FileError (Resource InputStream))
readViaStream descriptor offset = \token ->
  readViaStreamRaw descriptor offset

writeViaStream :: Resource Descriptor -> Int -> Effect (Either FileError (Resource OutputStream))
writeViaStream descriptor offset = \token ->
  writeViaStreamRaw descriptor offset

appendViaStream :: Resource Descriptor -> Effect (Either FileError (Resource OutputStream))
appendViaStream descriptor = \token ->
  appendViaStreamRaw descriptor

foreign import "wasi:filesystem/types#filesystem-error-code" filesystemErrorCodeRaw :: Resource Error -> Maybe FileError

-- | Recovers a filesystem `error-code` from a stream operation failure. The
-- | `Error` handle is borrowed by the WIT call.
filesystemErrorCode :: Resource Error -> Effect (Maybe FileError)
filesystemErrorCode err = \token -> filesystemErrorCodeRaw err

-- | Writes `contents` through an output stream. `Nothing` means the write
-- | succeeded; `Just` is the error from opening the stream. The `Either`
-- | returned by `blockingWriteAndFlush` is ignored here.
writeString :: Resource Descriptor -> String -> Effect (Maybe FileError)
writeString descriptor contents =
  bind (writeViaStream descriptor 0) (\opened ->
    case opened of
      Left err -> pure (Just err)
      Right stream ->
        bind (blockingWriteAndFlush stream contents) (\ignored ->
          bind (dropOutputStream stream) (\ignoredStream ->
            pure Nothing)))

-- | Reads up to `length` bytes through an input stream.
readString :: Resource Descriptor -> Int -> Effect (Either FileError String)
readString descriptor length =
  bind (readViaStream descriptor 0) (\opened ->
    case opened of
      Left err -> pure (Left err)
      Right stream ->
        bind (blockingRead stream length) (\contents ->
          bind (dropInputStream stream) (\ignored ->
            case contents of
              Right bytes -> pure (Right bytes)
              Left (LastOperationFailed err) ->
                bind (filesystemErrorCode err) (\code ->
                  bind (dropError err) (\ignoredError ->
                    case code of
                      Just fileError -> pure (Left fileError)
                      Nothing -> pure (Left Io)))
              Left Closed -> pure (Left Io))))

foreign import "wasi:filesystem/types#[method]descriptor.metadata-hash" metadataHashRaw :: Resource Descriptor -> Either FileError { lower :: Int, upper :: Int }
foreign import "wasi:filesystem/types#[method]descriptor.metadata-hash-at" metadataHashAtRaw :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Either FileError { lower :: Int, upper :: Int }

metadataHash :: Resource Descriptor -> Effect (Either FileError { lower :: Int, upper :: Int })
metadataHash descriptor = \token ->
  metadataHashRaw descriptor

metadataHashAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Effect (Either FileError { lower :: Int, upper :: Int })
metadataHashAt descriptor pathFlags path = \token ->
  metadataHashAtRaw descriptor pathFlags path

foreign import "wasi:filesystem/types#[method]descriptor.get-flags" getFlagsRaw :: Resource Descriptor -> Either FileError { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean }

getFlags :: Resource Descriptor -> Effect (Either FileError { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean })
getFlags descriptor = \token ->
  getFlagsRaw descriptor

foreign import "wasi:filesystem/types#[method]descriptor.get-type" getTypeRaw :: Resource Descriptor -> Either FileError DescriptorType

getType :: Resource Descriptor -> Effect (Either FileError DescriptorType)
getType descriptor = \token ->
  getTypeRaw descriptor

foreign import "wasi:filesystem/types#[method]descriptor.readlink-at" readlinkAtRaw :: Resource Descriptor -> String -> Either FileError String

readlinkAt :: Resource Descriptor -> String -> Effect (Either FileError String)
readlinkAt descriptor path = \token ->
  readlinkAtRaw descriptor path

foreign import "wasi:filesystem/types#[method]descriptor.is-same-object" isSameObjectRaw :: Resource Descriptor -> Resource Descriptor -> Boolean

isSameObject :: Resource Descriptor -> Resource Descriptor -> Effect Boolean
isSameObject left right = \token -> isSameObjectRaw left right

foreign import "wasi:filesystem/types#[method]descriptor.stat" statRaw :: Resource Descriptor -> Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } }
foreign import "wasi:filesystem/types#[method]descriptor.stat-at" statAtRaw :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } }

-- | The attributes of an open file or directory.
stat :: Resource Descriptor -> Effect (Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } })
stat descriptor = \token ->
  statRaw descriptor

-- | The attributes of a file or directory named by a relative path.
statAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Effect (Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } })
statAt descriptor pathFlags path = \token ->
  statAtRaw descriptor pathFlags path

foreign import "wasi:filesystem/types#[method]descriptor.read-directory" readDirectoryRaw :: Resource Descriptor -> Either FileError (Resource DirectoryEntryStream)
foreign import "wasi:filesystem/types#[method]directory-entry-stream.read-directory-entry" readDirectoryEntryRaw :: Resource DirectoryEntryStream -> Either FileError (Maybe { type :: DescriptorType, name :: String })
foreign import "wasi:filesystem/types#[resource-drop]directory-entry-stream" dropDirectoryEntryStreamRaw :: Resource DirectoryEntryStream -> Unit

-- | Opens a fresh stream over the entries of a directory.
readDirectory :: Resource Descriptor -> Effect (Either FileError (Resource DirectoryEntryStream))
readDirectory descriptor = \token ->
  readDirectoryRaw descriptor

-- | Reads the next entry from a directory stream. `Nothing` reports the end of
-- | the stream.
readDirectoryEntry :: Resource DirectoryEntryStream -> Effect (Either FileError (Maybe { type :: DescriptorType, name :: String }))
readDirectoryEntry stream = \token ->
  readDirectoryEntryRaw stream

dropDirectoryEntryStream :: Resource DirectoryEntryStream -> Effect Unit
dropDirectoryEntryStream stream = \token -> dropDirectoryEntryStreamRaw stream

-- The remaining operations report `result<_, error-code>`, which DEC-13 maps
-- to `Either FileError Unit`: `Right unit` on success, `Left` the error.
foreign import "wasi:filesystem/types#[method]descriptor.create-directory-at" createDirectoryAtRaw :: Resource Descriptor -> String -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.remove-directory-at" removeDirectoryAtRaw :: Resource Descriptor -> String -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.unlink-file-at" unlinkFileAtRaw :: Resource Descriptor -> String -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.rename-at" renameAtRaw :: Resource Descriptor -> String -> Resource Descriptor -> String -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.symlink-at" symlinkAtRaw :: Resource Descriptor -> String -> String -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.sync" syncRaw :: Resource Descriptor -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.sync-data" syncDataRaw :: Resource Descriptor -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.set-size" setSizeRaw :: Resource Descriptor -> Int -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.advise" adviseRaw :: Resource Descriptor -> Int -> Int -> Advice -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.link-at" linkAtRaw :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Resource Descriptor -> String -> Either FileError Unit

createDirectoryAt :: Resource Descriptor -> String -> Effect (Either FileError Unit)
createDirectoryAt descriptor path = \token -> createDirectoryAtRaw descriptor path

removeDirectoryAt :: Resource Descriptor -> String -> Effect (Either FileError Unit)
removeDirectoryAt descriptor path = \token -> removeDirectoryAtRaw descriptor path

unlinkFileAt :: Resource Descriptor -> String -> Effect (Either FileError Unit)
unlinkFileAt descriptor path = \token -> unlinkFileAtRaw descriptor path

renameAt :: Resource Descriptor -> String -> Resource Descriptor -> String -> Effect (Either FileError Unit)
renameAt descriptor oldPath newDescriptor newPath = \token ->
  renameAtRaw descriptor oldPath newDescriptor newPath

symlinkAt :: Resource Descriptor -> String -> String -> Effect (Either FileError Unit)
symlinkAt descriptor oldPath newPath = \token -> symlinkAtRaw descriptor oldPath newPath

sync :: Resource Descriptor -> Effect (Either FileError Unit)
sync descriptor = \token -> syncRaw descriptor

syncData :: Resource Descriptor -> Effect (Either FileError Unit)
syncData descriptor = \token -> syncDataRaw descriptor

setSize :: Resource Descriptor -> Int -> Effect (Either FileError Unit)
setSize descriptor size = \token -> setSizeRaw descriptor size

foreign import "wasi:filesystem/types#[method]descriptor.set-times" setTimesRaw :: Resource Descriptor -> NewTimestamp -> NewTimestamp -> Either FileError Unit
foreign import "wasi:filesystem/types#[method]descriptor.set-times-at" setTimesAtRaw :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> NewTimestamp -> NewTimestamp -> Either FileError Unit

-- | Adjusts the access and modification timestamps of an open file or
-- | directory. `Right unit` reports success; `Left` is the `FileError`.
setTimes :: Resource Descriptor -> NewTimestamp -> NewTimestamp -> Effect (Either FileError Unit)
setTimes descriptor accessTimestamp modificationTimestamp = \token ->
  setTimesRaw descriptor accessTimestamp modificationTimestamp

-- | Adjusts the timestamps of a file or directory named by a relative path.
setTimesAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> NewTimestamp -> NewTimestamp -> Effect (Either FileError Unit)
setTimesAt descriptor pathFlags path accessTimestamp modificationTimestamp = \token ->
  setTimesAtRaw descriptor pathFlags path accessTimestamp modificationTimestamp

advise :: Resource Descriptor -> Int -> Int -> Advice -> Effect (Either FileError Unit)
advise descriptor offset length advice = \token -> adviseRaw descriptor offset length advice

linkAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Resource Descriptor -> String -> Effect (Either FileError Unit)
linkAt descriptor pathFlags oldPath newDescriptor newPath = \token ->
  linkAtRaw descriptor pathFlags oldPath newDescriptor newPath

foreign import "wasi:filesystem/types#[resource-drop]descriptor" dropDescriptorRaw :: Resource Descriptor -> Unit

dropDescriptor :: Resource Descriptor -> Effect Unit
dropDescriptor descriptor = \token -> dropDescriptorRaw descriptor

-- | Runs `action` with `descriptor`, then drops it. The descriptor is always
-- | released once `action` completes.
withDescriptor :: forall a. Resource Descriptor -> (Resource Descriptor -> Effect a) -> Effect a
withDescriptor descriptor action =
  \token ->
    let result = action descriptor token in
    let ignored = dropDescriptor descriptor token in
    result
