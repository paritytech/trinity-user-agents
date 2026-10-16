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
        let fixture = try makeSocket()
        let (socket, connection) = (fixture.socket, fixture.connection)

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
        let fixture = try makeSocket()
        let (socket, connection) = (fixture.socket, fixture.connection)

        socket.open()
        socket.open()

        #expect(connection.connectCount == 1)
    }

    @Test func inboundFramesDeliveredVerbatimInOrder() throws {
        let fixture = try makeSocket()
        let (socket, delegate) = (fixture.socket, fixture.delegate)
        socket.open()
        socket.handle(.connected([:]))

        socket.handle(.text(#"{"id":1}"#))
        socket.handle(.binary(Data(#"{"id":2}"#.utf8)))
        socket.handle(.text(#"{"id":3}"#))

        #expect(delegate.received == [#"{"id":1}"#, #"{"id":2}"#, #"{"id":3}"#])
    }

    @Test(arguments: TransportDeath.allCases)
    func transportDeathReportsCloseExactlyOnce(death: TransportDeath) throws {
        let fixture = try makeSocket()
        let (socket, connection, delegate) = (fixture.socket, fixture.connection, fixture.delegate)
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
        let fixture = try makeSocket()
        let (socket, delegate) = (fixture.socket, fixture.delegate)
        socket.open()

        socket.handle(.error(URLError(.cannotConnectToHost)))

        #expect(delegate.closeReasons.count == 1)
        #expect(delegate.closeReasons.first?.isTransport == true)
    }

    @Test func callerCloseIsSilentAndFinal() throws {
        let fixture = try makeSocket()
        let (socket, connection, delegate) = (fixture.socket, fixture.connection, fixture.delegate)
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
        let fixture = try makeSocket()
        let (socket, connection) = (fixture.socket, fixture.connection)
        socket.open()
        socket.send(text: "pending")

        socket.handle(.error(nil))
        socket.handle(.connected([:]))

        #expect(connection.written.isEmpty)
    }

    @Test func missedPongClosesWithLivenessTimeout() throws {
        let fixture = try makeOpenSocket()

        fixture.scheduler.fire(after: Timing.pingInterval)
        #expect(fixture.connection.pingCount == 1)
        #expect(fixture.delegate.closeReasons.isEmpty)

        fixture.scheduler.fire(after: Timing.pongTimeout)

        #expect(fixture.delegate.closeReasons.count == 1)
        #expect(fixture.delegate.closeReasons.first?.isLivenessTimeout == true)
    }

    @Test func answeredPeriodicPingKeepsPinging() throws {
        let fixture = try makeOpenSocket()
        fixture.connection.onPing = { fixture.socket.handle(.pong(nil)) }

        fixture.scheduler.fire(after: Timing.pingInterval)
        fixture.scheduler.fire(after: Timing.pongTimeout)
        fixture.scheduler.fire(after: Timing.pingInterval)

        #expect(fixture.connection.pingCount == 2)
        #expect(fixture.delegate.closeReasons.isEmpty)
    }

    @Test func expireReportsResumedOnce() throws {
        let fixture = try makeOpenSocket()

        fixture.socket.expire()
        fixture.socket.handle(.error(nil))

        #expect(fixture.delegate.closeReasons.count == 1)
        #expect(fixture.delegate.closeReasons.first?.isResumed == true)
        #expect(fixture.connection.forceDisconnectCount == 1)
        #expect(fixture.scheduler.pendingDelays.isEmpty)
    }

    @Test func unansweredProbeClosesWithLivenessTimeout() throws {
        let fixture = try makeOpenSocket()

        fixture.socket.probe(timeout: Timing.probeTimeout)
        #expect(fixture.connection.pingCount == 1)
        #expect(fixture.delegate.closeReasons.isEmpty)

        fixture.scheduler.fire(after: Timing.probeTimeout)

        #expect(fixture.delegate.closeReasons.count == 1)
        #expect(fixture.delegate.closeReasons.first?.isLivenessTimeout == true)
    }

    @Test func answeredProbeKeepsSocketOpen() throws {
        let fixture = try makeOpenSocket()
        fixture.connection.onPing = { fixture.socket.handle(.pong(nil)) }

        fixture.socket.probe(timeout: Timing.probeTimeout)
        fixture.scheduler.fire(after: Timing.probeTimeout)

        #expect(fixture.connection.pingCount == 1)
        #expect(fixture.delegate.closeReasons.isEmpty)
    }

    @Test func probeIgnoredOnceClosed() throws {
        let fixture = try makeOpenSocket()
        fixture.socket.close()

        fixture.socket.probe(timeout: Timing.probeTimeout)

        #expect(fixture.connection.pingCount == 0)
        #expect(fixture.scheduler.pendingDelays.isEmpty)
    }

    @Test func stalledConnectClosesWithConnectTimeout() throws {
        let fixture = try makeSocket()
        fixture.socket.open()

        fixture.scheduler.fire(after: Timing.connectionTimeout)

        #expect(fixture.delegate.closeReasons.count == 1)
        #expect(fixture.delegate.closeReasons.first?.isConnectTimeout == true)
    }

    @Test func completedConnectCancelsConnectTimeout() throws {
        let fixture = try makeOpenSocket()

        fixture.scheduler.fire(after: Timing.connectionTimeout)

        #expect(fixture.delegate.closeReasons.isEmpty)
        #expect(fixture.scheduler.pendingDelays == [Timing.pingInterval])
    }
}

private extension ChainSocketTests {
    enum Timing {
        static let connectionTimeout: TimeInterval = 10
        static let pingInterval: TimeInterval = 30
        static let pongTimeout: TimeInterval = 15
        static let probeTimeout: TimeInterval = 5
    }

    struct Fixture {
        let socket: ChainSocket
        let connection: RecordingWebSocketConnection
        let delegate: RecordingChainSocketDelegate
        let scheduler: ManualChainSocketScheduler
    }

    func makeSocket() throws -> Fixture {
        let factory = RecordingTransportFactory()
        let scheduler = ManualChainSocketScheduler()
        let socket = try ChainSocket(
            url: #require(URL(string: "wss://node.example/rpc")),
            transportFactory: factory,
            configuration: .init(
                connectionTimeout: Timing.connectionTimeout,
                pingInterval: Timing.pingInterval,
                pongTimeout: Timing.pongTimeout
            ),
            scheduler: scheduler,
            logger: SilentLogger()
        )
        let delegate = RecordingChainSocketDelegate()
        socket.delegate = delegate

        let connection = try #require(factory.created.first?.connection)
        return Fixture(socket: socket, connection: connection, delegate: delegate, scheduler: scheduler)
    }

    func makeOpenSocket() throws -> Fixture {
        let fixture = try makeSocket()
        fixture.socket.open()
        fixture.socket.handle(.connected([:]))
        return fixture
    }
}
