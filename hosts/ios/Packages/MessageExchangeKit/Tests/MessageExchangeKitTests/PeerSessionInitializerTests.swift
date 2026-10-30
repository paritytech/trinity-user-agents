import CryptoKit
import Foundation
import Foundation_iOS
import StatementStore
import SubstrateSdk
import Testing
@testable import MessageExchangeKit

struct PeerSessionInitializerTests {
    let ownAccountId = Data(repeating: 1, count: 32)
    let peerAccountId = Data(repeating: 2, count: 32)
    let coder: StatementDataCoder

    init() throws {
        let encryptor = try X25519ChaChaPolyEncryptorFactory(privateKey: Curve25519.KeyAgreement.PrivateKey())
            .makeEncryptor(remotePublicKey: Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation)
        coder = StatementDataCoder(encryptor: encryptor, logger: nil)
    }

    @Test func junkWithMaxExpiryOnOwnTopicDoesNotSeedTheExpiry() async throws {
        let outcome = try await initialize(
            own: [statement(.request(request("r1")), from: ownAccountId, expiry: 500), junk(expiry: .max)],
            peer: []
        )

        let success = try #require(outcome.success)
        #expect(success.priority == 501)
        #expect(success.outgoingState.outgoingRequests[.device]?.requestId == "r1")
    }

    @Test func junkOnThePeerTopicKeepsThePeerRequestPending() async throws {
        let outcome = try await initialize(
            own: [],
            peer: [
                statement(.request(request("q1")), from: peerAccountId, expiry: 400),
                junk(expiry: 900, from: peerAccountId)
            ]
        )

        let success = try #require(outcome.success)
        #expect(success.incomingState.peerRequests.map(\.request.requestId) == ["q1"])
    }

    @Test func unparsableStatementOnTheTopicDoesNotFailInitialization() async throws {
        let outcome = try await initialize(
            own: [],
            peer: [statement(.request(request("q1")), from: peerAccountId, expiry: 400)],
            peerError: .statementDecodingFailed
        )

        let success = try #require(outcome.success)
        #expect(success.incomingState.peerRequests.map(\.request.requestId) == ["q1"])
    }

    @Test func transportPollingErrorFailsInitialization() async {
        let outcome = await initialize(own: [], peer: [], peerError: .other(PollingFailure()))

        #expect(outcome.success == nil)
        #expect(outcome.failure != nil)
    }
}

private extension PeerSessionInitializerTests {
    func initialize(
        own: [PeerSessionStatement],
        peer: [PeerSessionStatement],
        peerError: StatementSubscriptionError? = nil
    ) async -> InitializerOutcome {
        let initializer = PeerSessionInitializer<TestMessage>(
            priorityProvider: PeerSessionPriorityProvider(priorityFactory: FixedPriorityFactory(), logger: nil),
            ownSubscription: FixedSubscription(statements: own),
            peerSubscription: FixedSubscription(statements: peer, error: peerError),
            workQueue: DispatchQueue(label: "test.peer-session-initializer"),
            statementDataCoder: coder,
            supportedRoutes: [.device],
            logger: nil
        )
        let recorder = InitializerRecorder()
        initializer.delegate = AnyPeerSessionInitializerDelegate(recorder)

        return await withCheckedContinuation { continuation in
            recorder.onOutcome = { continuation.resume(returning: $0) }
            initializer.initializeSession()
        }
    }

    func request(_ requestId: String) -> MessageExchange.Request<TestMessage> {
        .init(requestId: requestId, messages: [TestMessage(id: 1)])
    }

    func statement(
        _ data: StatementData<TestMessage>,
        from topicAccountId: Data,
        expiry: UInt64
    ) throws -> PeerSessionStatement {
        let payload = try coder.encodeToScaleEncodedPayload(data, route: .device)
        return PeerSessionStatement(
            statement: [.expiry(expiry), .scaleEncodedPayload(payload)],
            route: .device,
            topicAccountId: topicAccountId
        )
    }

    /// What anyone can publish under a known topic: well-formed fields, but not encrypted with the session key.
    func junk(expiry: UInt64, from topicAccountId: Data? = nil) -> PeerSessionStatement {
        PeerSessionStatement(
            statement: [.expiry(expiry), .scaleEncodedPayload((try? Data(count: 64).scaleEncoded()) ?? Data())],
            route: .device,
            topicAccountId: topicAccountId ?? ownAccountId
        )
    }
}

struct InitializerOutcome {
    let success: SessionInitializationSuccess<TestMessage>?
    let failure: SessionInitializationFailure?
}

struct PollingFailure: Error {}

private final class InitializerRecorder: PeerSessionInitializerDelegate, TypeErasedDelegateStoring {
    var onOutcome: ((InitializerOutcome) -> Void)?

    func sessionInitializer(
        _: PeerSessionInitializing,
        didInitializeWith result: SessionInitializationSuccess<TestMessage>
    ) {
        onOutcome?(InitializerOutcome(success: result, failure: nil))
    }

    func sessionInitializer(
        _: PeerSessionInitializing,
        didFailToInitializeWith result: SessionInitializationFailure
    ) {
        onOutcome?(InitializerOutcome(success: nil, failure: result))
    }
}

private final class FixedSubscription: PeerSessionStatementSubscribing {
    let statements: [PeerSessionStatement]
    let error: StatementSubscriptionError?

    init(statements: [PeerSessionStatement], error: StatementSubscriptionError? = nil) {
        self.statements = statements
        self.error = error
    }

    func start(handler _: @escaping PeerSessionStatementHandler) {}
    func stop() {}
    func resetSeenHashes() {}

    func fetchOnce(
        handler: @escaping PeerSessionStatementHandler,
        completion: ((StatementSubscriptionError?) -> Void)?
    ) {
        statements.forEach { _ = handler($0) }
        completion?(error)
    }
}

final class FixedPriorityFactory: StatementPriorityMaking {
    func makeTimestampPriority() -> UInt64 { 100 }
}
