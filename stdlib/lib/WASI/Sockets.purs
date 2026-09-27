-- | Idiomatic wrappers over `wasi:sockets`. Raw imports stay module private;
-- | the public API is expressed with library types (`Network`, `TcpSocket`,
-- | `UdpSocket`, `Maybe`, `Either`, closed records, and ADTs).
-- |
-- | The address-returning methods (`local-address`, `remote-address`) and the
-- | datagram stream operation are exposed; their canonical memory layout is
-- | modeled by the backend.
module WASI.Sockets
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
import WASI.Streams (InputStream, OutputStream)

-- | An owned capability handle for (a subset of) the network.
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

foreign import "wasi:sockets/instance-network#instance-network" instanceNetworkRaw :: Network

instanceNetwork :: Effect Network
instanceNetwork = \token -> instanceNetworkRaw

foreign import "wasi:sockets/tcp-create-socket#create-tcp-socket" createTcpSocketRaw :: IpAddressFamily -> Either TcpSocket NetworkError
foreign import "wasi:sockets/udp-create-socket#create-udp-socket" createUdpSocketRaw :: IpAddressFamily -> Either UdpSocket NetworkError

createTcpSocket :: IpAddressFamily -> Effect (Either NetworkError TcpSocket)
createTcpSocket family = \token ->
  case createTcpSocketRaw family of
    Left socket -> Right socket
    Right err -> Left err

createUdpSocket :: IpAddressFamily -> Effect (Either NetworkError UdpSocket)
createUdpSocket family = \token ->
  case createUdpSocketRaw family of
    Left socket -> Right socket
    Right err -> Left err

foreign import "wasi:sockets/tcp#[method]tcp-socket.start-bind" tcpStartBindRaw :: TcpSocket -> Network -> IpSocketAddress -> Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-bind" tcpFinishBindRaw :: TcpSocket -> Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.start-connect" tcpStartConnectRaw :: TcpSocket -> Network -> IpSocketAddress -> Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.start-listen" tcpStartListenRaw :: TcpSocket -> Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-listen" tcpFinishListenRaw :: TcpSocket -> Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.shutdown" tcpShutdownRaw :: TcpSocket -> ShutdownType -> Unit
foreign import "wasi:sockets/tcp#[method]tcp-socket.finish-connect" tcpFinishConnectRaw :: TcpSocket -> Either { _1 :: InputStream, _2 :: OutputStream } NetworkError
foreign import "wasi:sockets/tcp#[method]tcp-socket.accept" tcpAcceptRaw :: TcpSocket -> Either { _1 :: TcpSocket, _2 :: InputStream, _3 :: OutputStream } NetworkError
foreign import "wasi:sockets/tcp#[method]tcp-socket.is-listening" tcpIsListeningRaw :: TcpSocket -> Boolean
foreign import "wasi:sockets/tcp#[method]tcp-socket.address-family" tcpAddressFamilyRaw :: TcpSocket -> IpAddressFamily
foreign import "wasi:sockets/tcp#[method]tcp-socket.local-address" tcpLocalAddressRaw :: TcpSocket -> Either IpSocketAddress NetworkError
foreign import "wasi:sockets/tcp#[method]tcp-socket.remote-address" tcpRemoteAddressRaw :: TcpSocket -> Either IpSocketAddress NetworkError
foreign import "wasi:sockets/tcp#[method]tcp-socket.keep-alive-idle-time" tcpKeepAliveIdleTimeRaw :: TcpSocket -> Either Int NetworkError

tcpStartBind :: TcpSocket -> Network -> IpSocketAddress -> Effect Unit
tcpStartBind socket network address = \token -> tcpStartBindRaw socket network address

tcpFinishBind :: TcpSocket -> Effect Unit
tcpFinishBind socket = \token -> tcpFinishBindRaw socket

tcpStartConnect :: TcpSocket -> Network -> IpSocketAddress -> Effect Unit
tcpStartConnect socket network address = \token -> tcpStartConnectRaw socket network address

tcpStartListen :: TcpSocket -> Effect Unit
tcpStartListen socket = \token -> tcpStartListenRaw socket

tcpFinishListen :: TcpSocket -> Effect Unit
tcpFinishListen socket = \token -> tcpFinishListenRaw socket

tcpShutdown :: TcpSocket -> ShutdownType -> Effect Unit
tcpShutdown socket how = \token -> tcpShutdownRaw socket how

tcpFinishConnect :: TcpSocket -> Effect (Either NetworkError { _1 :: InputStream, _2 :: OutputStream })
tcpFinishConnect socket = \token ->
  case tcpFinishConnectRaw socket of
    Left streams -> Right streams
    Right err -> Left err

tcpAccept :: TcpSocket -> Effect (Either NetworkError { _1 :: TcpSocket, _2 :: InputStream, _3 :: OutputStream })
tcpAccept socket = \token ->
  case tcpAcceptRaw socket of
    Left accepted -> Right accepted
    Right err -> Left err

tcpIsListening :: TcpSocket -> Effect Boolean
tcpIsListening socket = \token -> tcpIsListeningRaw socket

tcpAddressFamily :: TcpSocket -> Effect IpAddressFamily
tcpAddressFamily socket = \token -> tcpAddressFamilyRaw socket

tcpLocalAddress :: TcpSocket -> Effect (Either NetworkError IpSocketAddress)
tcpLocalAddress socket = \token ->
  case tcpLocalAddressRaw socket of
    Left address -> Right address
    Right err -> Left err

tcpRemoteAddress :: TcpSocket -> Effect (Either NetworkError IpSocketAddress)
tcpRemoteAddress socket = \token ->
  case tcpRemoteAddressRaw socket of
    Left address -> Right address
    Right err -> Left err

tcpKeepAliveIdleTime :: TcpSocket -> Effect (Either NetworkError Int)
tcpKeepAliveIdleTime socket = \token ->
  case tcpKeepAliveIdleTimeRaw socket of
    Left duration -> Right duration
    Right err -> Left err

foreign import "wasi:sockets/udp#[method]udp-socket.start-bind" udpStartBindRaw :: UdpSocket -> Network -> IpSocketAddress -> Unit
foreign import "wasi:sockets/udp#[method]udp-socket.finish-bind" udpFinishBindRaw :: UdpSocket -> Unit
foreign import "wasi:sockets/udp#[method]udp-socket.address-family" udpAddressFamilyRaw :: UdpSocket -> IpAddressFamily
foreign import "wasi:sockets/udp#[method]udp-socket.local-address" udpLocalAddressRaw :: UdpSocket -> Either IpSocketAddress NetworkError
foreign import "wasi:sockets/udp#[method]udp-socket.remote-address" udpRemoteAddressRaw :: UdpSocket -> Either IpSocketAddress NetworkError
foreign import "wasi:sockets/udp#[method]udp-socket.stream" udpSocketStreamRaw :: UdpSocket -> Maybe IpSocketAddress -> Either { _1 :: IncomingDatagramStream, _2 :: OutgoingDatagramStream } NetworkError

udpStartBind :: UdpSocket -> Network -> IpSocketAddress -> Effect Unit
udpStartBind socket network address = \token -> udpStartBindRaw socket network address

udpFinishBind :: UdpSocket -> Effect Unit
udpFinishBind socket = \token -> udpFinishBindRaw socket

udpAddressFamily :: UdpSocket -> Effect IpAddressFamily
udpAddressFamily socket = \token -> udpAddressFamilyRaw socket

udpLocalAddress :: UdpSocket -> Effect (Either NetworkError IpSocketAddress)
udpLocalAddress socket = \token ->
  case udpLocalAddressRaw socket of
    Left address -> Right address
    Right err -> Left err

udpRemoteAddress :: UdpSocket -> Effect (Either NetworkError IpSocketAddress)
udpRemoteAddress socket = \token ->
  case udpRemoteAddressRaw socket of
    Left address -> Right address
    Right err -> Left err

-- | Connects the UDP socket to an optional remote address and returns the
-- | incoming and outgoing datagram streams.
udpSocketStream :: UdpSocket -> Maybe IpSocketAddress -> Effect (Either NetworkError { _1 :: IncomingDatagramStream, _2 :: OutgoingDatagramStream })
udpSocketStream socket remote = \token ->
  case udpSocketStreamRaw socket remote of
    Left streams -> Right streams
    Right err -> Left err

foreign import "wasi:sockets/network#[resource-drop]network" dropNetworkRaw :: Network -> Unit
foreign import "wasi:sockets/tcp#[resource-drop]tcp-socket" dropTcpSocketRaw :: TcpSocket -> Unit
foreign import "wasi:sockets/udp#[resource-drop]udp-socket" dropUdpSocketRaw :: UdpSocket -> Unit
foreign import "wasi:sockets/udp#[resource-drop]incoming-datagram-stream" dropIncomingDatagramStreamRaw :: IncomingDatagramStream -> Unit
foreign import "wasi:sockets/udp#[resource-drop]outgoing-datagram-stream" dropOutgoingDatagramStreamRaw :: OutgoingDatagramStream -> Unit

dropNetwork :: Network -> Effect Unit
dropNetwork network = \token -> dropNetworkRaw network

dropTcpSocket :: TcpSocket -> Effect Unit
dropTcpSocket socket = \token -> dropTcpSocketRaw socket

dropUdpSocket :: UdpSocket -> Effect Unit
dropUdpSocket socket = \token -> dropUdpSocketRaw socket

dropIncomingDatagramStream :: IncomingDatagramStream -> Effect Unit
dropIncomingDatagramStream stream = \token -> dropIncomingDatagramStreamRaw stream

dropOutgoingDatagramStream :: OutgoingDatagramStream -> Effect Unit
dropOutgoingDatagramStream stream = \token -> dropOutgoingDatagramStreamRaw stream
