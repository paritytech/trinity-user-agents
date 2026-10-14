@testable import polkadot_app
import Foundation
import Foundation_iOS
import MessageExchangeKit
import SubstrateSdk
import StructuredConcurrency
import Testing

@Suite(.timeLimit(.minutes(1)))
struct DeviceSyncPeerEngineTests {
    @Test("Disposed engine ignores a stale connection failure")
    func disposedEngineIgnoresStaleConnectionFailure() async throws {
        let recorder = DeviceSyncPeerEngineRecorder()
        let engine = makeEngine(recorder: recorder)

        await engine.start()
        await recorder.waitForCreationCount(1)
        let exchange = try #require(recorder.latestExchange)
        await engine.dispose()
        await exchange.fail(.disconnected)

        #expect(recorder.creationCount == 1)
    }

    @Test("Exchange failure replaces the connection after ordered close")
    func failureClosesConnectionBeforeRetry() async throws {
        let recorder = DeviceSyncPeerEngineRecorder()
        let engine = makeEngine(recorder: recorder)

        await engine.start()
        await recorder.waitForCreationCount(1)
        let exchange = try #require(recorder.latestExchange)
        await exchange.fail(.disconnected)
        await recorder.waitForCreationCount(2)

        #expect(recorder.closedConnections == [1])
        #expect(recorder.createdBeforePreviousClosed == false)
        #expect(recorder.signalingSessionCreationCount == 1)
        #expect(recorder.latestTransport?.sentMessages.count == 2)
        await engine.dispose()
    }

    @Test("Accepted reconnect cancels a delayed failure replacement")
    func reconnectCancelsDelayedReplacement() async throws {
        let recorder = DeviceSyncPeerEngineRecorder()
        let sessionDelegate = DeviceSyncPeerSessionDelegate(peerLogger: MockLogger())
        let sleeper = ManualDeviceSyncSleeper()
        let engine = makeEngine(
            recorder: recorder,
            sessionDelegate: sessionDelegate,
            sleep: { duration in try await sleeper.sleep(for: duration) }
        )

        await engine.start()
        await recorder.waitForCreationCount(1)

        let session = try #require(recorder.latestSession)
        try await session.send([.offer(SdpCoderTests.validOfferSdp)])
        let transport = try #require(recorder.latestTransport)
        let offer = try #require(try transport.sentMessages.compactMap { data in
            let envelope = try Chat.DeviceSyncSignalingEnvelope(
                scaleDecoder: ScaleDecoder(data: data)
            )
            return envelope.message.isOffer ? envelope : nil
        }.last)

        let exchange = try #require(recorder.latestExchange)
        await exchange.fail(.disconnected)
        await sleeper.waitUntilSleeping()

        let reconnectTransport = MockDeviceSyncMessageTransport()
        let reconnectSignaler = DeviceSyncSignalingSession(
            transport: reconnectTransport,
            role: .acceptor,
            peerLogger: MockLogger()
        )
        await reconnectSignaler.sendReconnected(offerId: offer.offerId)
        let reconnectData = try #require(reconnectTransport.sentMessages.first)
        sessionDelegate.peerSession(
            MockDeviceSyncPeerSession(),
            didReceiveMessages: [reconnectData],
            respondHandler: { _ in }
        )

        await recorder.waitForCreationCount(2)
        sleeper.advance()

        #expect(await sleeper.waitForOutcome() == .cancelled)
        #expect(recorder.creationCount == 2)
        #expect(recorder.closedConnections == [1])
        await engine.dispose()
    }

    @Test("Reconnect waits for an in-progress replacement generation")
    func reconnectWaitsForInProgressReplacement() async throws {
        let creationGate = DeviceSyncExchangeCreationGate(blockedConnection: 2)
        let recorder = DeviceSyncPeerEngineRecorder(exchangeCreationGate: creationGate)
        let sessionDelegate = DeviceSyncPeerSessionDelegate(peerLogger: MockLogger())
        let engine = makeEngine(recorder: recorder, sessionDelegate: sessionDelegate)

        await engine.start()
        await recorder.waitForCreationCount(1)

        let session = try #require(recorder.latestSession)
        try await session.send([.offer(SdpCoderTests.validOfferSdp)])
        let transport = try #require(recorder.latestTransport)
        let offer = try #require(try transport.sentMessages.compactMap { data in
            let envelope = try Chat.DeviceSyncSignalingEnvelope(
                scaleDecoder: ScaleDecoder(data: data)
            )
            return envelope.message.isOffer ? envelope : nil
        }.last)

        let exchange = try #require(recorder.latestExchange)
        await exchange.fail(.disconnected)
        await recorder.waitForCreationCount(2)

        let reconnectTransport = MockDeviceSyncMessageTransport()
        let reconnectSignaler = DeviceSyncSignalingSession(
            transport: reconnectTransport,
            role: .acceptor,
            peerLogger: MockLogger()
        )
        await reconnectSignaler.sendReconnected(offerId: offer.offerId)
        let reconnectData = try #require(reconnectTransport.sentMessages.first)
        sessionDelegate.peerSession(
            MockDeviceSyncPeerSession(),
            didReceiveMessages: [reconnectData],
            respondHandler: { _ in }
        )

        await recorder.waitForCreationCount(3)

        #expect(recorder.closedConnections == [1, 2])
        #expect(recorder.createdBeforePreviousClosed == false)
        await engine.dispose()
    }

    @Test("New initiator offers wait for the acceptor's scheduled replacement")
    func newOffersWaitForScheduledAcceptorReplacement() async throws {
        let recorder = DeviceSyncPeerEngineRecorder(role: .acceptor)
        let offerIdRecorder = DeviceSyncOfferIdPersistenceRecorder()
        let sessionDelegate = DeviceSyncPeerSessionDelegate(peerLogger: MockLogger())
        let sleeper = ManualDeviceSyncSleeper()
        let engine = makeEngine(
            recorder: recorder,
            sessionDelegate: sessionDelegate,
            offerIdPersistenceHandler: { offerId in
                await offerIdRecorder.persist(offerId)
            },
            sleep: { duration in try await sleeper.sleep(for: duration) }
        )

        await engine.start()
        await recorder.waitForCreationCount(1)

        let sourceTransport = MockDeviceSyncMessageTransport()
        let initiator = DeviceSyncSignalingSession(
            transport: sourceTransport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        let offerData = sourceTransport.sentMessages
        let firstData = try #require(offerData.first)
        let latestData = try #require(offerData.last)
        let firstOffer = try Chat.DeviceSyncSignalingEnvelope(
            scaleDecoder: ScaleDecoder(data: firstData)
        )
        let latestOffer = try Chat.DeviceSyncSignalingEnvelope(
            scaleDecoder: ScaleDecoder(data: latestData)
        )
        let session = try #require(recorder.latestSession)
        await session.handleIncomingEnvelopes([firstOffer])
        let exchange = try #require(recorder.latestExchange)
        await exchange.fail(.disconnected)
        await sleeper.waitUntilSleeping()

        sessionDelegate.peerSession(
            MockDeviceSyncPeerSession(),
            didReceiveMessages: [latestData],
            respondHandler: { _ in }
        )

        sleeper.advance()
        await recorder.waitForCreationCount(2)
        await offerIdRecorder.waitForCount(2)

        #expect(await sleeper.waitForOutcome() == .completed)
        #expect(recorder.closedConnections == [1])
        #expect(recorder.createdBeforePreviousClosed == false)
        #expect(await offerIdRecorder.offerIds == [firstOffer.offerId, latestOffer.offerId])
        await engine.dispose()
    }

    @Test("Different peer engines start independently")
    func differentPeerEnginesStartIndependently() async throws {
        let firstRecorder = DeviceSyncPeerEngineRecorder()
        let secondRecorder = DeviceSyncPeerEngineRecorder()
        let first = makeEngine(peerByte: 1, recorder: firstRecorder)
        let second = makeEngine(peerByte: 2, recorder: secondRecorder)

        await first.start()
        await second.start()
        await firstRecorder.waitForCreationCount(1)
        await secondRecorder.waitForCreationCount(1)

        await first.dispose()
        await second.dispose()
    }

    @Test("Disposal during exchange creation closes the uninstalled generation")
    func disposalDuringExchangeCreationClosesGeneration() async throws {
        let creationGate = DeviceSyncExchangeCreationGate()
        let recorder = DeviceSyncPeerEngineRecorder(exchangeCreationGate: creationGate)
        let engine = makeEngine(recorder: recorder)

        await engine.start()
        await recorder.waitForCreationCount(1)

        let disposal = Task { await engine.dispose() }
        await recorder.waitForTransportClose()
        await creationGate.open()
        await disposal.value

        #expect(recorder.closedConnections == [1])
        #expect(await recorder.latestFlow?.didClose == true)
    }

    @Test("Accepted offer refreshes the reconnect fallback after durable persistence")
    func acceptedOfferRefreshesReconnectFallback() async throws {
        let recorder = DeviceSyncPeerEngineRecorder(role: .acceptor)
        let offerIdRecorder = DeviceSyncOfferIdPersistenceRecorder()
        let engine = makeEngine(
            recorder: recorder,
            persistedOfferId: "stale-offer",
            offerIdPersistenceHandler: { offerId in
                await offerIdRecorder.persist(offerId)
            }
        )

        await engine.start()
        await recorder.waitForCreationCount(1)

        let sourceTransport = MockDeviceSyncMessageTransport()
        let initiator = DeviceSyncSignalingSession(
            transport: sourceTransport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        let offerData = try #require(sourceTransport.sentMessages.first)
        let offer = try Chat.DeviceSyncSignalingEnvelope(
            scaleDecoder: ScaleDecoder(data: offerData)
        )
        let session = try #require(recorder.latestSession)
        await session.handleIncomingEnvelopes([offer])

        #expect(await offerIdRecorder.offerIds == [offer.offerId])

        let firstExchange = try #require(recorder.latestExchange)
        await firstExchange.fail(.disconnected)
        await recorder.waitForCreationCount(2)

        let secondExchange = try #require(recorder.latestExchange)
        await secondExchange.fail(.disconnected)
        await recorder.waitForCreationCount(3)

        let transport = try #require(recorder.latestTransport)
        let reconnectOfferIds = try transport.sentMessages.compactMap { data in
            let envelope = try Chat.DeviceSyncSignalingEnvelope(
                scaleDecoder: ScaleDecoder(data: data)
            )
            return envelope.message == .reconnected ? envelope.offerId : nil
        }
        #expect(reconnectOfferIds == ["stale-offer", offer.offerId, offer.offerId])

        await engine.dispose()
    }

    @Test("Unavailable dependencies retry the signaling lifecycle")
    func unavailableDependenciesRetrySignalingLifecycle() async {
        let recorder = DeviceSyncPeerEngineRecorder(
            signalingSessionError: DeviceSyncPeerEngineError.dependenciesUnavailable
        )
        let sleeper = ManualDeviceSyncSleeper()
        let engine = makeEngine(
            recorder: recorder,
            sleep: { duration in try await sleeper.sleep(for: duration) }
        )

        await engine.start()
        await recorder.waitForSignalingSessionCreationCount(1)
        await sleeper.waitUntilSleeping()
        sleeper.advance()
        await recorder.waitForSignalingSessionCreationCount(2)
        await recorder.waitForCreationCount(1)

        #expect(recorder.signalingSessionCreationCount == 2)
        #expect(recorder.creationCount == 1)
        await engine.dispose()
    }
}

private extension DeviceSyncPeerEngineTests {
    func makeEngine(
        peerByte: UInt8 = 1,
        recorder: DeviceSyncPeerEngineRecorder,
        sessionDelegate: DeviceSyncPeerSessionDelegate? = nil,
        persistedOfferId: String? = "persisted-offer",
        offerIdPersistenceHandler: @escaping @Sendable (String) async throws -> Void = { _ in },
        sleep: @escaping DeviceSyncSleep = { _ in }
    ) -> DeviceSyncPeerEngine {
        let delegate = sessionDelegate ?? DeviceSyncPeerSessionDelegate(peerLogger: MockLogger())
        let contextFactory = DeviceSyncPeerEngineContextFactory(
            componentFactory: recorder,
            sessionDelegate: delegate,
            persistedOfferId: persistedOfferId,
            offerIdPersistenceHandler: offerIdPersistenceHandler,
            restartDelayPolicy: DeviceSyncRestartDelayPolicy(),
            peerLogger: MockLogger(),
            sleep: sleep
        )
        return DeviceSyncPeerEngine(
            peerStatementAccountId: Data(repeating: peerByte, count: 32),
            contextFactory: contextFactory
        )
    }
}

private final class DeviceSyncPeerEngineRecorder: DeviceSyncPeerComponentMaking, @unchecked Sendable {
    private let role: CallRole
    private let exchangeCreationGate: DeviceSyncExchangeCreationGate?
    private let lock = NSLock()
    private let creationEvents = DeviceSyncTestEventRecorder<Int>()
    private let signalingSessionCreationEvents = DeviceSyncTestEventRecorder<Int>()
    private var _creationCount = 0
    private var _signalingSessionCreationCount = 0
    private var _closedConnections = [Int]()
    private var _createdBeforePreviousClosed = false
    private var _latestExchange: MockDeviceSyncExchange?
    private var _latestTransport: MockDeviceSyncMessageTransport?
    private var _latestSession: DeviceSyncSignalingSession?
    private var _signalingSessionError: Error?

    var creationCount: Int { lock.withLock { _creationCount } }
    var signalingSessionCreationCount: Int { lock.withLock { _signalingSessionCreationCount } }
    var closedConnections: [Int] { lock.withLock { _closedConnections } }
    var createdBeforePreviousClosed: Bool { lock.withLock { _createdBeforePreviousClosed } }
    var latestExchange: MockDeviceSyncExchange? { lock.withLock { _latestExchange } }
    var latestTransport: MockDeviceSyncMessageTransport? { lock.withLock { _latestTransport } }
    var latestSession: DeviceSyncSignalingSession? { lock.withLock { _latestSession } }
    var latestFlow: MockDeviceSyncPeerConnectionFlow? { lock.withLock { _latestFlow } }

    private var _latestFlow: MockDeviceSyncPeerConnectionFlow?

    init(
        role: CallRole = .initiator,
        exchangeCreationGate: DeviceSyncExchangeCreationGate? = nil,
        signalingSessionError: Error? = nil
    ) {
        self.role = role
        self.exchangeCreationGate = exchangeCreationGate
        _signalingSessionError = signalingSessionError
    }

    func makeSignalingSession(
        delegate: AnyPeerSessionDelegate<Data>,
        offerIdHandler: @escaping @Sendable (String) async throws -> Void
    ) async throws -> DeviceSyncSignalingSession {
        let (creationCount, error) = lock.withLock {
            _signalingSessionCreationCount += 1
            defer { _signalingSessionError = nil }
            return (_signalingSessionCreationCount, _signalingSessionError)
        }
        signalingSessionCreationEvents.record(creationCount)
        if let error { throw error }

        let transport = MockDeviceSyncMessageTransport()
        lock.withLock { _latestTransport = transport }
        try await transport.open(delegate: delegate)
        let session = DeviceSyncSignalingSession(
            transport: transport,
            role: role,
            offerIdHandler: offerIdHandler,
            peerLogger: MockLogger()
        )
        lock.withLock { _latestSession = session }
        return session
    }

    func makeConnectionFlow(
        session _: DeviceSyncSignalingSession
    ) -> any DeviceSyncPeerConnectionFlowing {
        let flow = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        lock.withLock {
            _creationCount += 1
            _latestFlow = flow
            if _creationCount > 1, !_closedConnections.contains(_creationCount - 1) {
                _createdBeforePreviousClosed = true
            }
        }
        return flow
    }

    func makeExchange(
        flow _: any DeviceSyncPeerConnectionFlowing,
        failureHandler: @escaping @Sendable (DeviceSyncConnectionFailure) async -> Void
    ) async throws -> any DeviceSyncExchanging {
        let connection = creationCount
        let exchange = MockDeviceSyncExchange(
            connection: connection,
            failureHandler: failureHandler,
            closeHandler: { [weak self] connection in
                self?.recordClose(connection)
            }
        )
        lock.withLock { _latestExchange = exchange }
        creationEvents.record(connection)
        await exchangeCreationGate?.wait(connection: connection)
        return exchange
    }

    func waitForCreationCount(_ expected: Int) async {
        _ = await creationEvents.waitForCount(expected)
    }

    func waitForSignalingSessionCreationCount(_ expected: Int) async {
        _ = await signalingSessionCreationEvents.waitForCount(expected)
    }

    func waitForTransportClose() async {
        await latestTransport?.waitUntilClosed()
    }

    private func recordClose(_ connection: Int) {
        lock.withLock {
            if !_closedConnections.contains(connection) { _closedConnections.append(connection) }
        }
    }
}

private actor DeviceSyncOfferIdPersistenceRecorder {
    private let events = DeviceSyncTestEventRecorder<String>()
    private(set) var offerIds = [String]()

    func persist(_ offerId: String) {
        offerIds.append(offerId)
        events.record(offerId)
    }

    func waitForCount(_ count: Int) async {
        _ = await events.waitForCount(count)
    }
}
