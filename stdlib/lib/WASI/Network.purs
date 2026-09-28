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

foreign import "wasi:sockets/instance-network#instance-network" instanceNetworkRaw :: Resource Network

instanceNetwork :: Effect (Resource Network)
instanceNetwork = \token -> instanceNetworkRaw

foreign import "wasi:sockets/tcp-create-socket#create-tcp-socket" createTcpSocketRaw :: IpAddressFamily -> Either NetworkError (Resource TcpSocket)
foreign import "wasi:sockets/udp-create-socket#create-udp-socket" createUdpSocketRaw :: IpAddressFamily -> Either NetworkError (Resource UdpSocket)

createTcpSocket :: IpAddressFamily -> Effect (Either NetworkError (Resource TcpSocket))
createTcpSocket family = \token ->
  createTcpSocketRaw family

createUdpSocket :: IpAddressFamily -> Effect (Either NetworkError (Resource UdpSocket))
createUdpSocket family = \token ->
  createUdpSocketRaw family

foreign import "wasi:sockets/tcp#[method]tcp-socket.start-bind" tcpStartBindRaw :: Resource TcpSocket -> Resource Network -> IpSocketAddress -> Either NetworkError Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-bind" tcpFinishBindRaw :: Resource TcpSocket -> Either NetworkError Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.start-connect" tcpStartConnectRaw :: Resource TcpSocket -> Resource Network -> IpSocketAddress -> Either NetworkError Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.start-listen" tcpStartListenRaw :: Resource TcpSocket -> Either NetworkError Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-listen" tcpFinishListenRaw :: Resource TcpSocket -> Either NetworkError Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.shutdown" tcpShutdownRaw :: Resource TcpSocket -> ShutdownType -> Either NetworkError Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-connect" tcpFinishConnectRaw :: Resource TcpSocket -> Either NetworkError { _1 :: Resource InputStream, _2 :: Resource OutputStream }
foreign import "wasi:sockets/tcp#[method]tcp-socket.accept" tcpAcceptRaw :: Resource TcpSocket -> Either NetworkError { _1 :: Resource TcpSocket, _2 :: Resource InputStream, _3 :: Resource OutputStream }
foreign import "wasi:sockets/tcp#[method]tcp-socket.is-listening" tcpIsListeningRaw :: Resource TcpSocket -> Boolean
foreign import "wasi:sockets/tcp#[method]tcp-socket.address-family" tcpAddressFamilyRaw :: Resource TcpSocket -> IpAddressFamily
foreign import "wasi:sockets/tcp#[method]tcp-socket.local-address" tcpLocalAddressRaw :: Resource TcpSocket -> Either NetworkError IpSocketAddress
foreign import "wasi:sockets/tcp#[method]tcp-socket.remote-address" tcpRemoteAddressRaw :: Resource TcpSocket -> Either NetworkError IpSocketAddress
foreign import "wasi:sockets/tcp#[method]tcp-socket.keep-alive-idle-time" tcpKeepAliveIdleTimeRaw :: Resource TcpSocket -> Either NetworkError Int

tcpStartBind :: Resource TcpSocket -> Resource Network -> IpSocketAddress -> Effect (Either NetworkError Unit)
tcpStartBind socket network address = \token -> tcpStartBindRaw socket network address

tcpFinishBind :: Resource TcpSocket -> Effect (Either NetworkError Unit)
tcpFinishBind socket = \token -> tcpFinishBindRaw socket

tcpStartConnect :: Resource TcpSocket -> Resource Network -> IpSocketAddress -> Effect (Either NetworkError Unit)
tcpStartConnect socket network address = \token -> tcpStartConnectRaw socket network address

tcpStartListen :: Resource TcpSocket -> Effect (Either NetworkError Unit)
tcpStartListen socket = \token -> tcpStartListenRaw socket

tcpFinishListen :: Resource TcpSocket -> Effect (Either NetworkError Unit)
tcpFinishListen socket = \token -> tcpFinishListenRaw socket

tcpShutdown :: Resource TcpSocket -> ShutdownType -> Effect (Either NetworkError Unit)
tcpShutdown socket how = \token -> tcpShutdownRaw socket how

tcpFinishConnect :: Resource TcpSocket -> Effect (Either NetworkError { _1 :: Resource InputStream, _2 :: Resource OutputStream })
tcpFinishConnect socket = \token ->
  tcpFinishConnectRaw socket

tcpAccept :: Resource TcpSocket -> Effect (Either NetworkError { _1 :: Resource TcpSocket, _2 :: Resource InputStream, _3 :: Resource OutputStream })
tcpAccept socket = \token ->
  tcpAcceptRaw socket

tcpIsListening :: Resource TcpSocket -> Effect Boolean
tcpIsListening socket = \token -> tcpIsListeningRaw socket

tcpAddressFamily :: Resource TcpSocket -> Effect IpAddressFamily
tcpAddressFamily socket = \token -> tcpAddressFamilyRaw socket

tcpLocalAddress :: Resource TcpSocket -> Effect (Either NetworkError IpSocketAddress)
tcpLocalAddress socket = \token ->
  tcpLocalAddressRaw socket

tcpRemoteAddress :: Resource TcpSocket -> Effect (Either NetworkError IpSocketAddress)
tcpRemoteAddress socket = \token ->
  tcpRemoteAddressRaw socket

tcpKeepAliveIdleTime :: Resource TcpSocket -> Effect (Either NetworkError Int)
tcpKeepAliveIdleTime socket = \token ->
  tcpKeepAliveIdleTimeRaw socket

foreign import "wasi:sockets/udp#[method]udp-socket.start-bind" udpStartBindRaw :: Resource UdpSocket -> Resource Network -> IpSocketAddress -> Either NetworkError Unit
foreign import "wasi:sockets/udp#[method]udp-socket.finish-bind" udpFinishBindRaw :: Resource UdpSocket -> Either NetworkError Unit
foreign import "wasi:sockets/udp#[method]udp-socket.address-family" udpAddressFamilyRaw :: Resource UdpSocket -> IpAddressFamily
foreign import "wasi:sockets/udp#[method]udp-socket.local-address" udpLocalAddressRaw :: Resource UdpSocket -> Either NetworkError IpSocketAddress
foreign import "wasi:sockets/udp#[method]udp-socket.remote-address" udpRemoteAddressRaw :: Resource UdpSocket -> Either NetworkError IpSocketAddress
foreign import "wasi:sockets/udp#[method]udp-socket.stream" udpSocketStreamRaw :: Resource UdpSocket -> Maybe IpSocketAddress -> Either NetworkError { _1 :: Resource IncomingDatagramStream, _2 :: Resource OutgoingDatagramStream }

udpStartBind :: Resource UdpSocket -> Resource Network -> IpSocketAddress -> Effect (Either NetworkError Unit)
udpStartBind socket network address = \token -> udpStartBindRaw socket network address

udpFinishBind :: Resource UdpSocket -> Effect (Either NetworkError Unit)
udpFinishBind socket = \token -> udpFinishBindRaw socket

udpAddressFamily :: Resource UdpSocket -> Effect IpAddressFamily
udpAddressFamily socket = \token -> udpAddressFamilyRaw socket

udpLocalAddress :: Resource UdpSocket -> Effect (Either NetworkError IpSocketAddress)
udpLocalAddress socket = \token ->
  udpLocalAddressRaw socket

udpRemoteAddress :: Resource UdpSocket -> Effect (Either NetworkError IpSocketAddress)
udpRemoteAddress socket = \token ->
  udpRemoteAddressRaw socket

-- | Connects the UDP socket to an optional remote address and returns the
-- | incoming and outgoing datagram streams.
udpSocketStream :: Resource UdpSocket -> Maybe IpSocketAddress -> Effect (Either NetworkError { _1 :: Resource IncomingDatagramStream, _2 :: Resource OutgoingDatagramStream })
udpSocketStream socket remote = \token ->
  udpSocketStreamRaw socket remote

foreign import "wasi:sockets/network#[resource-drop]network" dropNetworkRaw :: Resource Network -> Unit
foreign import "wasi:sockets/tcp#[resource-drop]tcp-socket" dropTcpSocketRaw :: Resource TcpSocket -> Unit
foreign import "wasi:sockets/udp#[resource-drop]udp-socket" dropUdpSocketRaw :: Resource UdpSocket -> Unit
foreign import "wasi:sockets/udp#[resource-drop]incoming-datagram-stream" dropIncomingDatagramStreamRaw :: Resource IncomingDatagramStream -> Unit
foreign import "wasi:sockets/udp#[resource-drop]outgoing-datagram-stream" dropOutgoingDatagramStreamRaw :: Resource OutgoingDatagramStream -> Unit

dropNetwork :: Resource Network -> Effect Unit
dropNetwork network = \token -> dropNetworkRaw network

dropTcpSocket :: Resource TcpSocket -> Effect Unit
dropTcpSocket socket = \token -> dropTcpSocketRaw socket

dropUdpSocket :: Resource UdpSocket -> Effect Unit
dropUdpSocket socket = \token -> dropUdpSocketRaw socket

dropIncomingDatagramStream :: Resource IncomingDatagramStream -> Effect Unit
dropIncomingDatagramStream stream = \token -> dropIncomingDatagramStreamRaw stream

dropOutgoingDatagramStream :: Resource OutgoingDatagramStream -> Effect Unit
dropOutgoingDatagramStream stream = \token -> dropOutgoingDatagramStreamRaw stream
