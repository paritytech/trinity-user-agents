import CryptoKit
import Foundation
import MessageExchangeKit
import NovaCrypto
import StatementStore
import SubstrateSdk
import Testing
@testable import polkadot_app

@Suite(.timeLimit(.minutes(10)))
struct PerChatSessionSigningTests {
    @Test func signsDeviceRouteOnlyWithPerChatAccounts() async throws {
        let store = MockStatementStore()
        let alice = try Participant()
        let bob = try Participant()

        try await deliverText(from: alice, to: bob, store: store)

        let senders = try Set(store.senderAccountIds())

        #expect(senders == [alice.statementAccountId, bob.statementAccountId])
    }

    @Test func deliversWhenSenderSignsWithIdentityAccount() async throws {
        let store = MockStatementStore()
        let alice = try Participant(signsWithIdentity: true)
        let bob = try Participant()

        try await deliverText(from: alice, to: bob, store: store)

        let senders = try Set(store.senderAccountIds())

        #expect(senders == [alice.identityAccountId, bob.statementAccountId])
    }

    @Test func noSessionUntilPeerDevicesAreKnown() async throws {
        let store = MockStatementStore()
        let alice = try Participant()
        let bob = try Participant()
        let aliceService = try alice.makeService(store: store)
        let request = alice.request(for: bob, knowingDevices: false)

        var didPost = false
        alice.delegate.didPostMessagesClosure = { _, _, _ in didPost = true }

        aliceService.updateSessions([request])
        aliceService.addMessagesToQueue([alice.textMessage()], for: request.peer)

        try await Task.sleep(for: .milliseconds(500))

        #expect(!didPost)
        #expect(try store.senderAccountIds().isEmpty)
    }
}

private extension PerChatSessionSigningTests {
    func deliverText(from alice: Participant, to bob: Participant, store: MockStatementStore) async throws {
        let aliceService = try alice.makeService(store: store)
        let bobService = try bob.makeService(store: store)

        aliceService.updateSessions([alice.request(for: bob, knowingDevices: true)])
        bobService.updateSessions([bob.request(for: alice, knowingDevices: true)])

        bob.delegate.didReceiveMessagesClosure = { _, _, completion in completion(.success) }

        await withCheckedContinuation { continuation in
            alice.delegate.didDeliverMessagesClosure = { _, _, _ in continuation.resume() }
            aliceService.addMessagesToQueue(
                [alice.textMessage()],
                for: alice.request(for: bob, knowingDevices: true).peer
            )
        }
    }
}

private struct Participant {
    let identityKeypair: SNKeypairProtocol
    let statementKeypair: SNKeypairProtocol
    let identityEncryptionKey = Curve25519.KeyAgreement.PrivateKey()
    let deviceEncryptionKey = Curve25519.KeyAgreement.PrivateKey()
    let delegate = MockChatParticipant()
    let signsWithIdentity: Bool

    init(signsWithIdentity: Bool = false) throws {
        self.signsWithIdentity = signsWithIdentity
        identityKeypair = try SNKeyFactory().createKeypair(fromSeed: Data.randomOrError(of: 32))
        statementKeypair = try SNKeyFactory().createKeypair(fromSeed: Data.randomOrError(of: 32))
    }

    var identityAccountId: AccountId { identityKeypair.publicKey().rawData() }
    var statementAccountId: AccountId { statementKeypair.publicKey().rawData() }

    func makeService(store: MockStatementStore) throws -> AnyMessageExchangeService<Chat.OpaqueMessage> {
        let signManager = ClosureSignerManager { [identityKeypair] _ in
            StatementStoreKeypairSigner(keypair: identityKeypair)
        }

        return try MessageExchangeServiceFactory(
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .multidevice),
            signManager: signManager,
            encryptionManager: ClosureEncryptionManager { [identityEncryptionKey] _ in
                X25519ChaChaPolyEncryptorFactory(privateKey: identityEncryptionKey)
            },
            deviceEncryptionKeyFactory: X25519ChaChaPolyEncryptorFactory(privateKey: deviceEncryptionKey),
            maxStatementSize: 4_096
        )
        .makeService(
            statementStoreConnection: store,
            statementSignerResolver: StubSignerResolver(signer: StatementStoreKeypairSigner(
                keypair: signsWithIdentity ? identityKeypair : statementKeypair
            )),
            delegate: AnyPeerSessionDelegate(delegate)
        )
    }

    func request(for peer: Participant, knowingDevices: Bool) -> MessageExchange.SessionRequest {
        let devices = knowingDevices ? [
            MessageExchange.DeviceInfo(
                statementAccountId: peer.identityAccountId,
                encryptionPublicKey: peer.deviceEncryptionKey.publicKey.rawRepresentation
            )
        ] : []

        return MessageExchange.SessionRequest(
            own: .init(signKeyId: "identity", encryptionKeyId: "", pin: nil),
            peer: .init(
                accountId: peer.identityAccountId,
                publicKey: peer.identityEncryptionKey.publicKey.rawRepresentation,
                pin: nil,
                devices: devices
            )
        )
    }

    func textMessage() -> Chat.OpaqueMessage {
        .init(remoteMessage: Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: Date().toChatTimestamp(),
            versioned: .v1(.init(content: .text("hello")))
        ))
    }
}

private struct StubSignerResolver: PeerSessionSignerResolving, @unchecked Sendable {
    let signer: StatementStoreSigning

    func signer(for _: MessageExchange.SessionRequest) async throws -> StatementStoreSigning {
        signer
    }
}
