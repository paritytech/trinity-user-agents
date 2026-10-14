@testable import polkadot_app
import XCTest
import Keystore_iOS
import SubstrateSdk
import NovaCrypto
import MessageExchangeKit
import StatementStore
import CryptoKit
import SDKLogger

final class ChatIntegrationTest: XCTestCase {
    let seedAlice = Data(repeating: 1, count: 32)
    let seedBob = Data(repeating: 2, count: 32)

    static let statementStoreURL = URL(string: "wss://previewnet.substrate.dev/people")!

    struct ParticipantService {
        let request: MessageExchange.SessionRequest
        let messageExchange: AnyMessageExchangeService<Chat.OpaqueMessage>
    }

    func testCanSendAndReceive() throws {
        let logger: polkadot_app.LoggerProtocol = Logger.shared

        let alice = MockChatParticipant()
        let aliceParticipant = AnyPeerSessionDelegate(alice)
        let aliceService = try setupParticipantService(seed: seedAlice, participant: aliceParticipant)

        let bob = MockChatParticipant()
        let bobParticipant = AnyPeerSessionDelegate(bob)
        let bobService = try setupParticipantService(seed: seedBob, participant: bobParticipant)

        logger.debug("Alice: \(aliceService.request.peer.accountId.toHex())")
        logger.debug("Bob: \(bobService.request.peer.accountId.toHex())")

        // setup connection

        aliceService.messageExchange.updateSessions([bobService.request])
        bobService.messageExchange.updateSessions([aliceService.request])

        // send message

        bob.didReceiveMessagesClosure = { _, _, completion in
            logger.debug("Did receive message")
            completion(.success)
        }

        let message = Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: Date().toChatTimestamp(),
            versioned: .v1(.init(content: .text("Hey Bob! It was a great game!")))
        )

        let postExpectation = XCTestExpectation()
        let deliverExpectation = XCTestExpectation()

        alice.didPostMessagesClosure = { _, _, error in
            if let error {
                XCTFail("Unexpected error: \(error)")
            } else {
                logger.debug("Messages posted")
            }
            postExpectation.fulfill()
        }

        alice.didDeliverMessagesClosure = { _, _, error in
            if let error {
                XCTFail("Unexpected error: \(error)")
            } else {
                logger.debug("Messages delivered")
            }
            deliverExpectation.fulfill()
        }

        aliceService.messageExchange.addMessagesToQueue([.init(remoteMessage: message)], for: bobService.request.peer)

        wait(for: [postExpectation, deliverExpectation], timeout: 10)

        logger.debug("Completed: \(alice) \(bob)")
    }
}

private extension ChatIntegrationTest {
    func setupParticipantService(
        seed: Data,
        participant: AnyPeerSessionDelegate<Chat.OpaqueMessage>
    ) throws -> ParticipantService {
        let keypair = try SNKeyFactory().createKeypair(fromSeed: seed)
        let accountId = keypair.publicKey().rawData()
        Logger.shared.info("AccountId: \(accountId.toHex(includePrefix: true))")
        let connection = WebSocketEngine(urls: [Self.statementStoreURL], logger: Logger.shared)!
        let privateKey = Curve25519.KeyAgreement.PrivateKey()

        let encryptionManager = ClosureEncryptionManager { _ in
            X25519ChaChaPolyEncryptorFactory(privateKey: privateKey)
        }

        let signManager = ClosureSignerManager { _ in
            StatementStoreKeypairSigner(keypair: keypair)
        }

        let statementStore = StatementStoreConnection(
            connection: connection,
            retryMatcher: StatementSubmitTimeoutMatcher(),
            logger: Logger.shared
        )

        let messageExchangeService = try MessageExchangeServiceFactory(
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .identity),
            signManager: signManager,
            encryptionManager: encryptionManager,
            deviceEncryptionKeyFactory: nil,
            messageRouteSelector: { _ in .identity },
            maxStatementSize: 1_024
        ).makeService(
            statementStoreConnection: statementStore,
            delegate: participant
        )

        return ParticipantService(
            request: MessageExchange.SessionRequest(
                own: MessageExchange.Own(signKeyId: "", encryptionKeyId: "", pin: nil),
                peer: MessageExchange.Peer(
                    accountId: accountId,
                    publicKey: privateKey.publicKey.rawRepresentation,
                    pin: nil,
                    devices: []
                )
            ),
            messageExchange: messageExchangeService
        )
    }
}
