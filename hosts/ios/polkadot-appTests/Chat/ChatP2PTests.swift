@testable import polkadot_app
import XCTest
import NovaCrypto
import MessageExchangeKit
import StatementStore
import CryptoKit

final class ChatP2PTests: XCTestCase {
    struct ParticipantService {
        let request: MessageExchange.SessionRequest
        let messageExchange: AnyMessageExchangeService<Chat.OpaqueMessage>
    }

    func testCanSendAndReceive() throws {
        let statementStore = MockStatementStore()
        let logger: polkadot_app.LoggerProtocol = Logger.shared

        let alice = MockChatParticipant()
        let aliceService = try setupParticipantService(
            with: statementStore,
            participant: AnyPeerSessionDelegate(alice)
        )

        let bob = MockChatParticipant()
        let bobService = try setupParticipantService(
            with: statementStore,
            participant: AnyPeerSessionDelegate(bob)
        )

        logger.debug("Alice: \(aliceService.request.peer.accountId.toHex())")
        logger.debug("Bob: \(bobService.request.peer.accountId.toHex())")

        // setup connection

        aliceService.messageExchange.updateSessions([bobService.request])
        bobService.messageExchange.updateSessions([aliceService.request])

        // send message

        bob.didReceiveMessagesClosure = { _, _, completion in
            logger.debug("Did receive messages")
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

        aliceService.messageExchange.addMessagesToQueue(
            [.init(remoteMessage: message)],
            for: bobService.request.peer
        )

        wait(for: [postExpectation, deliverExpectation], timeout: 10)

        logger.debug("Completed: \(alice) \(bob)")
    }
}

private extension ChatP2PTests {
    func setupParticipantService(
        with statementStore: StatementStoreConnecting,
        participant: AnyPeerSessionDelegate<Chat.OpaqueMessage>
    ) throws -> ParticipantService {
        let seed = Data.random(of: 32)!
        let keypair = try SNKeyFactory().createKeypair(fromSeed: seed)
        let privateKey = Curve25519.KeyAgreement.PrivateKey()

        let encryptionManager = ClosureEncryptionManager { _ in
            X25519ChaChaPolyEncryptorFactory(privateKey: privateKey)
        }

        let signManager = ClosureSignerManager { _ in
            StatementStoreKeypairSigner(keypair: keypair)
        }

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
                own: .init(signKeyId: "", encryptionKeyId: "", pin: nil),
                peer: .init(
                    accountId: keypair.publicKey().rawData(),
                    publicKey: privateKey.publicKey.rawRepresentation,
                    pin: nil,
                    devices: []
                )
            ),
            messageExchange: messageExchangeService
        )
    }
}
