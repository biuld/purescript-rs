-- | Idiomatic wrappers over `wasi:filesystem`. Raw imports stay module
-- | private; the public API is expressed with library types (`Descriptor`,
-- | `Maybe`, `Either`, closed records, and an `error-code` ADT).
module WASI.Filesystem
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
import WASI.Streams (Error, InputStream, OutputStream, StreamError(..), blockingRead, blockingWriteAndFlush, dropError, dropInputStream, dropOutputStream)

-- | An owned filesystem descriptor. Drop it with `dropDescriptor`.
foreign import data Descriptor :: Type

-- | An owned stream of directory entries, from `readDirectory`. Drop it with
-- | `dropDirectoryEntryStream`.
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
-- | succeeded; `Just` is the error from opening the stream. The `Either`
-- | returned by `blockingWriteAndFlush` is ignored here.
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

foreign import "wasi:filesystem/types#[method]descriptor.stat" statRaw :: Descriptor -> Either { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } } FileError
foreign import "wasi:filesystem/types#[method]descriptor.stat-at" statAtRaw :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Either { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } } FileError

-- | The attributes of an open file or directory.
stat :: Descriptor -> Effect (Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } })
stat descriptor = \token ->
  case statRaw descriptor of
    Left attributes -> Right attributes
    Right err -> Left err

-- | The attributes of a file or directory named by a relative path.
statAt :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Effect (Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } })
statAt descriptor pathFlags path = \token ->
  case statAtRaw descriptor pathFlags path of
    Left attributes -> Right attributes
    Right err -> Left err

foreign import "wasi:filesystem/types#[method]descriptor.read-directory" readDirectoryRaw :: Descriptor -> Either DirectoryEntryStream FileError
foreign import "wasi:filesystem/types#[method]directory-entry-stream.read-directory-entry" readDirectoryEntryRaw :: DirectoryEntryStream -> Either (Maybe { type :: DescriptorType, name :: String }) FileError
foreign import "wasi:filesystem/types#[resource-drop]directory-entry-stream" dropDirectoryEntryStreamRaw :: DirectoryEntryStream -> Unit

-- | Opens a fresh stream over the entries of a directory.
readDirectory :: Descriptor -> Effect (Either FileError DirectoryEntryStream)
readDirectory descriptor = \token ->
  case readDirectoryRaw descriptor of
    Left stream -> Right stream
    Right err -> Left err

-- | Reads the next entry from a directory stream. `Nothing` reports the end of
-- | the stream.
readDirectoryEntry :: DirectoryEntryStream -> Effect (Either FileError (Maybe { type :: DescriptorType, name :: String }))
readDirectoryEntry stream = \token ->
  case readDirectoryEntryRaw stream of
    Left entry -> Right entry
    Right err -> Left err

dropDirectoryEntryStream :: DirectoryEntryStream -> Effect Unit
dropDirectoryEntryStream stream = \token -> dropDirectoryEntryStreamRaw stream

-- The remaining operations report `result<_, error-code>`, which DEC-13 maps
-- to `Either Unit FileError`: `Left unit` on success, `Right` the error.
foreign import "wasi:filesystem/types#[method]descriptor.create-directory-at" createDirectoryAtRaw :: Descriptor -> String -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.remove-directory-at" removeDirectoryAtRaw :: Descriptor -> String -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.unlink-file-at" unlinkFileAtRaw :: Descriptor -> String -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.rename-at" renameAtRaw :: Descriptor -> String -> Descriptor -> String -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.symlink-at" symlinkAtRaw :: Descriptor -> String -> String -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.sync" syncRaw :: Descriptor -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.sync-data" syncDataRaw :: Descriptor -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.set-size" setSizeRaw :: Descriptor -> Int -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.advise" adviseRaw :: Descriptor -> Int -> Int -> Advice -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.link-at" linkAtRaw :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Descriptor -> String -> Either Unit FileError

createDirectoryAt :: Descriptor -> String -> Effect (Either Unit FileError)
createDirectoryAt descriptor path = \token -> createDirectoryAtRaw descriptor path

removeDirectoryAt :: Descriptor -> String -> Effect (Either Unit FileError)
removeDirectoryAt descriptor path = \token -> removeDirectoryAtRaw descriptor path

unlinkFileAt :: Descriptor -> String -> Effect (Either Unit FileError)
unlinkFileAt descriptor path = \token -> unlinkFileAtRaw descriptor path

renameAt :: Descriptor -> String -> Descriptor -> String -> Effect (Either Unit FileError)
renameAt descriptor oldPath newDescriptor newPath = \token ->
  renameAtRaw descriptor oldPath newDescriptor newPath

symlinkAt :: Descriptor -> String -> String -> Effect (Either Unit FileError)
symlinkAt descriptor oldPath newPath = \token -> symlinkAtRaw descriptor oldPath newPath

sync :: Descriptor -> Effect (Either Unit FileError)
sync descriptor = \token -> syncRaw descriptor

syncData :: Descriptor -> Effect (Either Unit FileError)
syncData descriptor = \token -> syncDataRaw descriptor

setSize :: Descriptor -> Int -> Effect (Either Unit FileError)
setSize descriptor size = \token -> setSizeRaw descriptor size

foreign import "wasi:filesystem/types#[method]descriptor.set-times" setTimesRaw :: Descriptor -> NewTimestamp -> NewTimestamp -> Either Unit FileError
foreign import "wasi:filesystem/types#[method]descriptor.set-times-at" setTimesAtRaw :: Descriptor -> { symlinkFollow :: Boolean } -> String -> NewTimestamp -> NewTimestamp -> Either Unit FileError

-- | Adjusts the access and modification timestamps of an open file or
-- | directory. `Left unit` reports success; `Right` is the `FileError`.
setTimes :: Descriptor -> NewTimestamp -> NewTimestamp -> Effect (Either Unit FileError)
setTimes descriptor accessTimestamp modificationTimestamp = \token ->
  setTimesRaw descriptor accessTimestamp modificationTimestamp

-- | Adjusts the timestamps of a file or directory named by a relative path.
setTimesAt :: Descriptor -> { symlinkFollow :: Boolean } -> String -> NewTimestamp -> NewTimestamp -> Effect (Either Unit FileError)
setTimesAt descriptor pathFlags path accessTimestamp modificationTimestamp = \token ->
  setTimesAtRaw descriptor pathFlags path accessTimestamp modificationTimestamp

advise :: Descriptor -> Int -> Int -> Advice -> Effect (Either Unit FileError)
advise descriptor offset length advice = \token -> adviseRaw descriptor offset length advice

linkAt :: Descriptor -> { symlinkFollow :: Boolean } -> String -> Descriptor -> String -> Effect (Either Unit FileError)
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
