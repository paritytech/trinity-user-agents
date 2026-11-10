import Foundation
import MessageExchangeKit
import StatementDelivery
import StatementStore

enum ChatAcceptanceStatementBuilderError: Error {
    case notRemote
}

/// The acceptance as an identity-route request outside the chat session, encrypted with the identity keys.
struct ChatAcceptanceStatementBuilder: @unchecked Sendable {
    let encryptionManager: MessageExchangeEncryptionManaging
    let signerManager: StatementStoreSignerManaging
    var sessionIdFactory: PeerSessionIdFactoryProtocol = PeerSessionIdFactory()
    var channelFactory: StatementChannelMaking = ChatStatementChannelFactory()
    var priorityFactory: StatementPriorityMaking = StatementPriorityFactory()
}

extension ChatAcceptanceStatementBuilder: OutgoingStatementBuilding {
    func makeUnsignedStatement(for input: OutgoingChatAcceptanceInput) throws -> UnsignedStatement {
        guard let remoteMessage = input.message.toRemote() else {
            throw ChatAcceptanceStatementBuilderError.notRemote
        }

        let encryptor = try makeEncryptor(for: input.session)
        let sessionId = try makeSessionId(for: input.session, encryptor: encryptor)
        let request = MessageExchange.Request(
            requestId: input.message.messageId,
            messages: [Chat.OpaqueMessage(remoteMessage: remoteMessage)]
        )

        let payload = try StatementDataCoder(encryptor: encryptor, logger: nil).encodeToScaleEncodedPayload(
            StatementData.request(request),
            route: .identity
        )

        return try UnsignedStatement()
            .addTopic1(sessionId.own)
            .addChannel(channelFactory.createRequestChannel(for: sessionId))
            .addExpiry(priorityFactory.makeTimestampPriority())
            .addScaleEncodedPayload(payload)
    }

    func encodedSize(of input: OutgoingChatAcceptanceInput) throws -> Int {
        try makeUnsignedStatement(for: input).signedSize()
    }

    // The acceptor publishes nothing else on this topic, so a statement from a copy signer is the acceptance.
    func storageTopicFilter(for input: OutgoingChatAcceptanceInput) throws -> TopicFilter {
        let sessionId = try makeSessionId(for: input.session, encryptor: makeEncryptor(for: input.session))
        return try .matchAll([sessionId.own.fixedStatementFieldData()])
    }
}

private extension ChatAcceptanceStatementBuilder {
    func makeEncryptor(for session: MessageExchange.SessionRequest) throws -> MessageExchangeEncrypting {
        try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: session.own.encryptionKeyId)
            .makeEncryptor(remotePublicKey: session.peer.publicKey)
    }

    func makeSessionId(
        for session: MessageExchange.SessionRequest,
        encryptor: MessageExchangeEncrypting
    ) throws -> MessageExchange.SessionId {
        try sessionIdFactory.createSessionId(
            for: .init(
                ownAccountId: signerManager.makeSigner(for: session.own.signKeyId).accountId,
                ownPin: session.own.pin,
                peerAccountId: session.peer.accountId,
                peerPin: session.peer.pin,
                sharedSecret: encryptor.sharedSecret
            )
        )
    }
}
