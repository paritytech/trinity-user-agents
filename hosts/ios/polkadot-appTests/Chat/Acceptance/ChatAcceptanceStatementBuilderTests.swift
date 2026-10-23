import CryptoKit
import Foundation
import MessageExchangeKit
import StatementStore
import SubstrateSdk
import Testing
@testable import polkadot_app

struct ChatAcceptanceStatementBuilderTests {
    private let fixture = ChatAcceptanceFixture()
    private let identityEncryptionKey = Curve25519.KeyAgreement.PrivateKey()

    @Test func storedCopiesAreLookedUpOnTheOwnSessionTopic() throws {
        let builder = makeBuilder()
        let input = makeInput()

        let statement = try Statement(scaleDecoder: ScaleDecoder(data: builder.makeUnsignedStatement(for: input)
                .signed(by: StubStatementSigner(accountId: Data(repeating: 1, count: 32)))
                .build()
                .encodedStatement))

        guard case let .matchAll(topics) = try builder.storageTopicFilter(for: input) else {
            Issue.record("Expected a match-all filter")
            return
        }

        #expect(topics == [statement.getTopic1()].compactMap { $0 })
    }

    @Test func encodedSizeIsTheSignedStatementSize() throws {
        let builder = makeBuilder()
        let input = makeInput()

        let signed = try builder.makeUnsignedStatement(for: input)
            .signed(by: StubStatementSigner(accountId: Data(repeating: 1, count: 32)))
            .build()

        #expect(try builder.encodedSize(of: input) == signed.encodedStatement.count)
    }
}

private extension ChatAcceptanceStatementBuilderTests {
    func makeBuilder() -> ChatAcceptanceStatementBuilder {
        ChatAcceptanceStatementBuilder(
            encryptionManager: ClosureEncryptionManager { [identityEncryptionKey] _ in
                X25519ChaChaPolyEncryptorFactory(privateKey: identityEncryptionKey)
            },
            signerManager: ChatSignerManager(entropyManager: FixedRootEntropyManager())
        )
    }

    // A real peer key, since the identity route derives its topic from the shared secret.
    func makeInput() -> OutgoingChatAcceptanceInput {
        let contact = fixture.makeContact()
        let peerKey = Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation
        let peerContact = Chat.Contact(
            accountId: contact.accountId,
            username: contact.username,
            publicKey: peerKey,
            pin: nil,
            pushId: nil,
            pushToken: nil,
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: contact.ownKeyId,
            imageData: nil,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )

        return OutgoingChatAcceptanceInput(
            message: fixture.makeMessage(),
            requestId: fixture.requestId,
            contact: peerContact
        )
    }
}
