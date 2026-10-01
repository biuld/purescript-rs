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

foreign import "wasi:filesystem/preopens#get-directories" preopens :: Effect (Array { _1 :: Resource Descriptor, _2 :: String })

-- | The preopened directories. The canonical ABI fixes a tuple's field names
-- | to `_1`/`_2`, so each element carries the descriptor first and its path
-- | second; `preopen` returns the friendlier named record for one entry.
-- | The preopened directory at `index`, as a named record.
preopen :: Int -> Effect { descriptor :: Resource Descriptor, path :: String }
preopen index =
  map (\entries ->
    let entry = arrayIndex entries index in
    { descriptor: entry._1, path: entry._2 }
  ) preopens

foreign import "wasi:filesystem/types#[method]descriptor.open-at" openAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> { create :: Boolean, directory :: Boolean, exclusive :: Boolean, truncate :: Boolean } -> { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean } -> Effect (Either FileError (Resource Descriptor))

openRead :: Resource Descriptor -> String -> Effect (Either FileError (Resource Descriptor))
openRead descriptor path =
  openAt descriptor defaultPathFlags path defaultOpenFlags (defaultDescriptorFlags { read = true })

openWrite :: Resource Descriptor -> String -> Effect (Either FileError (Resource Descriptor))
openWrite descriptor path =
  openAt descriptor defaultPathFlags path (defaultOpenFlags { create = true, truncate = true }) (defaultDescriptorFlags { write = true })

openAppend :: Resource Descriptor -> String -> Effect (Either FileError (Resource Descriptor))
openAppend descriptor path =
  openAt descriptor defaultPathFlags path defaultOpenFlags (defaultDescriptorFlags { write = true })

-- | Reads up to `length` bytes at `offset`. A WIT `list<u8>` is `Array Int`,
-- | not `String`, so the payload carries uninterpreted bytes
-- | ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
foreign import "wasi:filesystem/types#[method]descriptor.read" readRaw :: Resource Descriptor -> Int -> Int -> Effect (Either FileError { _1 :: Array Int, _2 :: Boolean })

-- | Reads up to `length` bytes at `offset`. `Right` is the decoded byte
-- | sequence, `Left` a `FileError`.
readFile :: Resource Descriptor -> Int -> Int -> Effect (Either FileError String)
readFile descriptor length offset =
  bind (readRaw descriptor length offset) (\result ->
    case result of
      Right value ->
        let bytes = value._1
        in pure (Right (bytesToString bytes))
      Left err -> pure (Left err))

foreign import "wasi:filesystem/types#[method]descriptor.write" writeFile :: Resource Descriptor -> Array Int -> Int -> Effect (Either FileError Int)

foreign import "wasi:filesystem/types#[method]descriptor.read-via-stream" readViaStream :: Resource Descriptor -> Int -> Effect (Either FileError (Resource InputStream))
foreign import "wasi:filesystem/types#[method]descriptor.write-via-stream" writeViaStream :: Resource Descriptor -> Int -> Effect (Either FileError (Resource OutputStream))
foreign import "wasi:filesystem/types#[method]descriptor.append-via-stream" appendViaStream :: Resource Descriptor -> Effect (Either FileError (Resource OutputStream))

foreign import "wasi:filesystem/types#filesystem-error-code" filesystemErrorCode :: Resource Error -> Effect (Maybe FileError)

-- | Recovers a filesystem `error-code` from a stream operation failure. The
-- | `Error` handle is borrowed by the WIT call.
-- | Writes `contents` through an output stream. `Nothing` means the write
-- | succeeded; `Just` is the error from opening the stream. The `Either`
-- | returned by `blockingWriteAndFlush` is ignored here.
writeString :: Resource Descriptor -> String -> Effect (Maybe FileError)
writeString descriptor contents =
  bind (writeViaStream descriptor 0) (\opened ->
    case opened of
      Left err -> pure (Just err)
      Right stream ->
        bind (blockingWriteAndFlush stream (stringToBytes contents)) (\ignored ->
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
              Right bytes -> pure (Right (bytesToString bytes))
              Left (LastOperationFailed err) ->
                bind (filesystemErrorCode err) (\code ->
                  bind (dropError err) (\ignoredError ->
                    case code of
                      Just fileError -> pure (Left fileError)
                      Nothing -> pure (Left Io)))
              Left Closed -> pure (Left Io))))

foreign import "wasi:filesystem/types#[method]descriptor.metadata-hash" metadataHash :: Resource Descriptor -> Effect (Either FileError { lower :: Int, upper :: Int })
foreign import "wasi:filesystem/types#[method]descriptor.metadata-hash-at" metadataHashAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Effect (Either FileError { lower :: Int, upper :: Int })

foreign import "wasi:filesystem/types#[method]descriptor.get-flags" getFlags :: Resource Descriptor -> Effect (Either FileError { read :: Boolean, write :: Boolean, fileIntegritySync :: Boolean, dataIntegritySync :: Boolean, requestedWriteSync :: Boolean, mutateDirectory :: Boolean })

foreign import "wasi:filesystem/types#[method]descriptor.get-type" getType :: Resource Descriptor -> Effect (Either FileError DescriptorType)

foreign import "wasi:filesystem/types#[method]descriptor.readlink-at" readlinkAt :: Resource Descriptor -> String -> Effect (Either FileError String)

foreign import "wasi:filesystem/types#[method]descriptor.is-same-object" isSameObject :: Resource Descriptor -> Resource Descriptor -> Effect (Boolean)

foreign import "wasi:filesystem/types#[method]descriptor.stat" stat :: Resource Descriptor -> Effect (Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } })
foreign import "wasi:filesystem/types#[method]descriptor.stat-at" statAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Effect (Either FileError { type :: DescriptorType, linkCount :: Int, size :: Int, dataAccessTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, dataModificationTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int }, statusChangeTimestamp :: Maybe { seconds :: Int, nanoseconds :: Int } })

-- | The attributes of an open file or directory.
-- | The attributes of a file or directory named by a relative path.
foreign import "wasi:filesystem/types#[method]descriptor.read-directory" readDirectory :: Resource Descriptor -> Effect (Either FileError (Resource DirectoryEntryStream))
foreign import "wasi:filesystem/types#[method]directory-entry-stream.read-directory-entry" readDirectoryEntry :: Resource DirectoryEntryStream -> Effect (Either FileError (Maybe { type :: DescriptorType, name :: String }))
foreign import "wasi:filesystem/types#[resource-drop]directory-entry-stream" dropDirectoryEntryStream :: Resource DirectoryEntryStream -> Effect (Unit)

-- | Opens a fresh stream over the entries of a directory.
-- | Reads the next entry from a directory stream. `Nothing` reports the end of
-- | the stream.
-- The remaining operations report `result<_, error-code>`, which DEC-13 maps
-- to `Either FileError Unit`: `Right unit` on success, `Left` the error.
foreign import "wasi:filesystem/types#[method]descriptor.create-directory-at" createDirectoryAt :: Resource Descriptor -> String -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.remove-directory-at" removeDirectoryAt :: Resource Descriptor -> String -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.unlink-file-at" unlinkFileAt :: Resource Descriptor -> String -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.rename-at" renameAt :: Resource Descriptor -> String -> Resource Descriptor -> String -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.symlink-at" symlinkAt :: Resource Descriptor -> String -> String -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.sync" sync :: Resource Descriptor -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.sync-data" syncData :: Resource Descriptor -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.set-size" setSize :: Resource Descriptor -> Int -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.advise" advise :: Resource Descriptor -> Int -> Int -> Advice -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.link-at" linkAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> Resource Descriptor -> String -> Effect (Either FileError Unit)

foreign import "wasi:filesystem/types#[method]descriptor.set-times" setTimes :: Resource Descriptor -> NewTimestamp -> NewTimestamp -> Effect (Either FileError Unit)
foreign import "wasi:filesystem/types#[method]descriptor.set-times-at" setTimesAt :: Resource Descriptor -> { symlinkFollow :: Boolean } -> String -> NewTimestamp -> NewTimestamp -> Effect (Either FileError Unit)

-- | Adjusts the access and modification timestamps of an open file or
-- | directory. `Right unit` reports success; `Left` is the `FileError`.
-- | Adjusts the timestamps of a file or directory named by a relative path.
foreign import "wasi:filesystem/types#[resource-drop]descriptor" dropDescriptor :: Resource Descriptor -> Effect (Unit)

-- | Runs `action` with `descriptor`, then drops it. The descriptor is always
-- | released once `action` completes.
withDescriptor :: forall a. Resource Descriptor -> (Resource Descriptor -> Effect a) -> Effect a
withDescriptor descriptor action =
  bind (action descriptor) \result ->
    bind (dropDescriptor descriptor) \_ ->
      pure result
