import CryptoKit
import Foundation
import MessageExchangeKit
import StatementStore
import SubstrateSdk
import Testing
@testable import polkadot_app

struct ChatRequestStatementBuilderTests {
    private let entropyManager = FixedRootEntropyManager()

    @Test func storedCopiesAreLookedUpOnTheRequestChannel() throws {
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

        #expect(topics == [statement.getTopic3()].compactMap { $0 })
    }

    @Test func encodedSizeCoversPayloadAndThreeTopics() throws {
        let builder = makeBuilder()
        let input = makeInput()

        let payload = try Statement(scaleDecoder: ScaleDecoder(data: builder.makeUnsignedStatement(for: input)
                .signed(by: StubStatementSigner(accountId: Data(repeating: 1, count: 32)))
                .build()
                .encodedStatement)).getScaleEncodedPayload()

        #expect(try builder.encodedSize(of: input) == (payload?.count ?? 0) + StatementSize.overhead(topicCount: 3))
    }
}

private extension ChatRequestStatementBuilderTests {
    func makeBuilder() -> ChatRequestStatementBuilder {
        let encryptionManager = ChatEncryptionManager(entropyManager: entropyManager)
        let signManager = ChatSignerManager(entropyManager: entropyManager)

        return ChatRequestStatementBuilder(
            requestFactory: ChatRequestFactory(
                encryptionManager: encryptionManager,
                signManager: signManager,
                remoteContactResolver: MockRemoteContactResolver(result: .success(nil))
            ),
            channelFactory: ChatRequestChannelFactory(
                encryptionManager: encryptionManager,
                signingManager: signManager,
                sessionIdFactory: PeerSessionIdFactory()
            ),
            priorityFactory: StatementPriorityFactory()
        )
    }

    func makeInput() -> OutgoingChatRequestInput {
        OutgoingChatRequestInput(
            message: Chat.RequestMessage(
                messageId: "request",
                timestamp: 1,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            session: MessageExchange.SessionRequest(
                own: .init(signKeyId: WalletDerivationPath.main(for: "dot"), encryptionKeyId: "chat", pin: nil),
                peer: .init(
                    accountId: Data(repeating: 0x0A, count: 32),
                    publicKey: Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation,
                    pin: nil,
                    devices: []
                )
            )
        )
    }
}
