import Testing
import Foundation
import Starscream
@testable import ChainRegistry

struct ChainSocketTests {
    enum TransportDeath: CaseIterable, Sendable {
        case peerClosed
        case error
        case cancelled
        case reconnectSuggested

        var event: WebSocketEvent {
            switch self {
            case .peerClosed: .disconnected("bye", 1_000)
            case .error: .error(nil)
            case .cancelled: .cancelled
            case .reconnectSuggested: .reconnectSuggested(true)
            }
        }
    }

    @Test func framesSentBeforeConnectFlushInOrder() throws {
        let (socket, connection, _) = try makeSocket()

        socket.open()
        socket.send(text: "a")
        socket.send(text: "b")
        #expect(connection.written.isEmpty)

        socket.handle(.connected([:]))
        socket.send(text: "c")

        #expect(connection.connectCount == 1)
        #expect(connection.written == ["a", "b", "c"])
    }

    @Test func openIsIdempotent() throws {
        let (socket, connection, _) = try makeSocket()

        socket.open()
        socket.open()

        #expect(connection.connectCount == 1)
    }

    @Test func inboundFramesDeliveredVerbatimInOrder() throws {
        let (socket, _, delegate) = try makeSocket()
        socket.open()
        socket.handle(.connected([:]))

        socket.handle(.text(#"{"id":1}"#))
        socket.handle(.binary(Data(#"{"id":2}"#.utf8)))
        socket.handle(.text(#"{"id":3}"#))

        #expect(delegate.received == [#"{"id":1}"#, #"{"id":2}"#, #"{"id":3}"#])
    }

    @Test(arguments: TransportDeath.allCases)
    func transportDeathReportsCloseExactlyOnce(death: TransportDeath) throws {
        let (socket, connection, delegate) = try makeSocket()
        socket.open()
        socket.handle(.connected([:]))

        socket.handle(death.event)
        socket.handle(.error(nil))
        socket.handle(.text("late"))

        let reason = try #require(delegate.closeReasons.first)
        #expect(delegate.closeReasons.count == 1)
        #expect(reason.isPeerClosed || reason.isTransport)
        #expect(delegate.received.isEmpty)
        #expect(connection.forceDisconnectCount == 1)
    }

    @Test func connectFailureReportsClose() throws {
        let (socket, _, delegate) = try makeSocket()
        socket.open()

        socket.handle(.error(URLError(.cannotConnectToHost)))

        #expect(delegate.closeReasons.count == 1)
        #expect(delegate.closeReasons.first?.isTransport == true)
    }

    @Test func callerCloseIsSilentAndFinal() throws {
        let (socket, connection, delegate) = try makeSocket()
        socket.open()
        socket.handle(.connected([:]))

        socket.close()
        socket.handle(.disconnected("bye", 1_000))
        socket.handle(.text("late"))
        socket.send(text: "dropped")
        socket.open()

        #expect(delegate.closeReasons.isEmpty)
        #expect(delegate.received.isEmpty)
        #expect(connection.written.isEmpty)
        #expect(connection.connectCount == 1)
        #expect(connection.forceDisconnectCount == 1)
    }

    @Test func pendingFramesDroppedOnClose() throws {
        let (socket, connection, _) = try makeSocket()
        socket.open()
        socket.send(text: "pending")

        socket.handle(.error(nil))
        socket.handle(.connected([:]))

        #expect(connection.written.isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func missedPongClosesWithLivenessTimeout() async throws {
        let configuration = ChainSocket.Configuration(connectionTimeout: 1, pingInterval: 0.05, pongTimeout: 0.05)
        let (socket, connection, delegate) = try makeSocket(configuration: configuration)

        let reason = await withCheckedContinuation { continuation in
            delegate.onClose = { continuation.resume(returning: $0) }
            socket.open()
            socket.handle(.connected([:]))
        }

        #expect(reason.isLivenessTimeout)
        #expect(connection.pingCount >= 1)
        #expect(delegate.closeReasons.count == 1)
    }
}

private extension ChainSocketTests {
    func makeSocket(
        configuration: ChainSocket.Configuration = .default
    ) throws -> (ChainSocket, RecordingWebSocketConnection, RecordingChainSocketDelegate) {
        let factory = RecordingTransportFactory()
        let socket = try ChainSocket(
            url: #require(URL(string: "wss://node.example/rpc")),
            transportFactory: factory,
            configuration: configuration,
            logger: SilentLogger()
        )
        let delegate = RecordingChainSocketDelegate()
        socket.delegate = delegate

        let connection = try #require(factory.created.first?.connection)
        return (socket, connection, delegate)
    }
}
