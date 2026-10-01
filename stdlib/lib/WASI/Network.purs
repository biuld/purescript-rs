-- | Idiomatic wrappers over `wasi:sockets`. Raw imports stay module private;
-- | the public API is expressed with `Resource a` handles and library types
-- | (`Maybe`, `Either`, closed records, and ADTs). Every `result` maps to
-- | `Either` with the error on `Left`.
-- |
-- | The address-returning methods (`local-address`, `remote-address`) and the
-- | datagram stream operation are exposed; their canonical memory layout is
-- | modeled by the backend.
module WASI.Network
  ( Network
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

import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe)
import WASI.IO (InputStream, OutputStream)
import WASI.Resource (Resource)

-- | Opaque phantom types naming the socket resources a `Resource` may wrap.
foreign import data Network :: Type
foreign import data TcpSocket :: Type
foreign import data UdpSocket :: Type
foreign import data IncomingDatagramStream :: Type
foreign import data OutgoingDatagramStream :: Type

-- | `error-code`, mapped from the WIT enum.
data NetworkError
  = Unknown
  | AccessDenied
  | NotSupported
  | InvalidArgument
  | OutOfMemory
  | Timeout
  | ConcurrencyConflict
  | NotInProgress
  | WouldBlock
  | InvalidState
  | NewSocketLimit
  | AddressNotBindable
  | AddressInUse
  | RemoteUnreachable
  | ConnectionRefused
  | ConnectionReset
  | ConnectionAborted
  | DatagramTooLarge
  | NameUnresolvable
  | TemporaryResolverFailure
  | PermanentResolverFailure

-- | `ip-address-family`, mapped from the WIT enum.
data IpAddressFamily = Ipv4 | Ipv6

-- | `ip-socket-address`, mapped from the WIT variant. The constructor names
-- | are library-chosen; the ABI matches variant cases by position.
data IpSocketAddress
  = IpV4SocketAddress { port :: Int, address :: { _1 :: Int, _2 :: Int, _3 :: Int, _4 :: Int } }
  | IpV6SocketAddress { port :: Int, flowInfo :: Int, address :: { _1 :: Int, _2 :: Int, _3 :: Int, _4 :: Int, _5 :: Int, _6 :: Int, _7 :: Int, _8 :: Int }, scopeId :: Int }

-- | `shutdown-type`, mapped from the WIT enum.
data ShutdownType = Receive | Send | Both

foreign import "wasi:sockets/instance-network#instance-network" instanceNetwork :: Effect (Resource Network)

foreign import "wasi:sockets/tcp-create-socket#create-tcp-socket" createTcpSocket :: IpAddressFamily -> Effect (Either NetworkError (Resource TcpSocket))
foreign import "wasi:sockets/udp-create-socket#create-udp-socket" createUdpSocket :: IpAddressFamily -> Effect (Either NetworkError (Resource UdpSocket))

foreign import "wasi:sockets/tcp#[method]tcp-socket.start-bind" tcpStartBind :: Resource TcpSocket -> Resource Network -> IpSocketAddress -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-bind" tcpFinishBind :: Resource TcpSocket -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/tcp#[method]tcp-socket.start-connect" tcpStartConnect :: Resource TcpSocket -> Resource Network -> IpSocketAddress -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/tcp#[method]tcp-socket.start-listen" tcpStartListen :: Resource TcpSocket -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-listen" tcpFinishListen :: Resource TcpSocket -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/tcp#[method]tcp-socket.shutdown" tcpShutdown :: Resource TcpSocket -> ShutdownType -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-connect" tcpFinishConnect :: Resource TcpSocket -> Effect (Either NetworkError { _1 :: Resource InputStream, _2 :: Resource OutputStream })
foreign import "wasi:sockets/tcp#[method]tcp-socket.accept" tcpAccept :: Resource TcpSocket -> Effect (Either NetworkError { _1 :: Resource TcpSocket, _2 :: Resource InputStream, _3 :: Resource OutputStream })
foreign import "wasi:sockets/tcp#[method]tcp-socket.is-listening" tcpIsListening :: Resource TcpSocket -> Effect (Boolean)
foreign import "wasi:sockets/tcp#[method]tcp-socket.address-family" tcpAddressFamily :: Resource TcpSocket -> Effect (IpAddressFamily)
foreign import "wasi:sockets/tcp#[method]tcp-socket.local-address" tcpLocalAddress :: Resource TcpSocket -> Effect (Either NetworkError IpSocketAddress)
foreign import "wasi:sockets/tcp#[method]tcp-socket.remote-address" tcpRemoteAddress :: Resource TcpSocket -> Effect (Either NetworkError IpSocketAddress)
foreign import "wasi:sockets/tcp#[method]tcp-socket.keep-alive-idle-time" tcpKeepAliveIdleTime :: Resource TcpSocket -> Effect (Either NetworkError Int)

foreign import "wasi:sockets/udp#[method]udp-socket.start-bind" udpStartBind :: Resource UdpSocket -> Resource Network -> IpSocketAddress -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/udp#[method]udp-socket.finish-bind" udpFinishBind :: Resource UdpSocket -> Effect (Either NetworkError Unit)
foreign import "wasi:sockets/udp#[method]udp-socket.address-family" udpAddressFamily :: Resource UdpSocket -> Effect (IpAddressFamily)
foreign import "wasi:sockets/udp#[method]udp-socket.local-address" udpLocalAddress :: Resource UdpSocket -> Effect (Either NetworkError IpSocketAddress)
foreign import "wasi:sockets/udp#[method]udp-socket.remote-address" udpRemoteAddress :: Resource UdpSocket -> Effect (Either NetworkError IpSocketAddress)
foreign import "wasi:sockets/udp#[method]udp-socket.stream" udpSocketStream :: Resource UdpSocket -> Maybe IpSocketAddress -> Effect (Either NetworkError { _1 :: Resource IncomingDatagramStream, _2 :: Resource OutgoingDatagramStream })

-- | Connects the UDP socket to an optional remote address and returns the
-- | incoming and outgoing datagram streams.
foreign import "wasi:sockets/network#[resource-drop]network" dropNetwork :: Resource Network -> Effect (Unit)
foreign import "wasi:sockets/tcp#[resource-drop]tcp-socket" dropTcpSocket :: Resource TcpSocket -> Effect (Unit)
foreign import "wasi:sockets/udp#[resource-drop]udp-socket" dropUdpSocket :: Resource UdpSocket -> Effect (Unit)
foreign import "wasi:sockets/udp#[resource-drop]incoming-datagram-stream" dropIncomingDatagramStream :: Resource IncomingDatagramStream -> Effect (Unit)
foreign import "wasi:sockets/udp#[resource-drop]outgoing-datagram-stream" dropOutgoingDatagramStream :: Resource OutgoingDatagramStream -> Effect (Unit)

