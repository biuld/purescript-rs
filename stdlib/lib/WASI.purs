-- | The WASI umbrella. A program that wants the common capabilities can
-- | `import WASI` and use the consolidated public API. The filesystem and
-- | socket surfaces are re-exported explicitly so their `error-code` enums do
-- | not collide; their constructors stay available from the focused modules.
module WASI
  ( module WASI.Resource
  , module WASI.IO
  , module WASI.Console
  , module WASI.Clock
  , module WASI.Random
  , module WASI.Process
  , Descriptor
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
  , Network
  , TcpSocket
  , UdpSocket
  , IncomingDatagramStream
  , OutgoingDatagramStream
  , NetworkError(..)
  , IpAddressFamily(..)
  , IpSocketAddress(..)
  , ShutdownType(..)
  , instanceNetwork
  , createTcpSocket
  , createUdpSocket
  , tcpStartBind
  , tcpFinishBind
  , tcpStartConnect
  , tcpFinishConnect
  , tcpStartListen
  , tcpFinishListen
  , tcpAccept
  , tcpIsListening
  , tcpAddressFamily
  , tcpLocalAddress
  , tcpRemoteAddress
  , tcpShutdown
  , tcpKeepAliveIdleTime
  , udpStartBind
  , udpFinishBind
  , udpAddressFamily
  , udpLocalAddress
  , udpRemoteAddress
  , udpSocketStream
  , dropNetwork
  , dropTcpSocket
  , dropUdpSocket
  , dropIncomingDatagramStream
  , dropOutgoingDatagramStream
  ) where

import WASI.Resource
import WASI.IO
import WASI.Console
import WASI.Clock
import WASI.Random
import WASI.Process
import WASI.FileSystem
import WASI.Network
