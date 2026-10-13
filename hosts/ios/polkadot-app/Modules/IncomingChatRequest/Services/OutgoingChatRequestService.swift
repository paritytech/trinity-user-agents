import Foundation
import SubstrateSdk
import MessageExchangeKit
import StatementStore
import Operation_iOS
import SDKLogger

protocol OutgoingChatRequestServicing {
    /// Only the outer statement is signed by `signer`; the inner proof and the session stay on `ownKeyId`.
    func send(
        message: Chat.RequestMessage,
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own,
        signer: StatementStoreSigning
    ) async throws

    func encodedSize(
        of message: Chat.RequestMessage,
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> Int

    func isStored(
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own,
        signedBy accountId: AccountId
    ) async throws -> Bool
}

enum OutgoingChatRequestServiceError: Error {
    case unexpected(String)
}

final class OutgoingChatRequestService {
    private static let topicCount = 3

    private let statementStoreConnection: StatementStoreConnecting
    private let priorityFactory: StatementPriorityMaking
    private let requestFactory: ChatRequestFactoryProtocol
    private let channelFactory: ChatRequestChannelFactoryProtocol
    private let logger: SDKLoggerProtocol

    init(
        statementStoreConnection: StatementStoreConnecting,
        requestFactory: ChatRequestFactoryProtocol,
        priorityFactory: StatementPriorityMaking,
        channelFactory: ChatRequestChannelFactoryProtocol,
        logger: SDKLoggerProtocol
    ) {
        self.statementStoreConnection = statementStoreConnection
        self.priorityFactory = priorityFactory
        self.channelFactory = channelFactory
        self.requestFactory = requestFactory
        self.logger = logger
    }
}

extension OutgoingChatRequestService: OutgoingChatRequestServicing {
    func send(
        message: Chat.RequestMessage,
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own,
        signer: StatementStoreSigning
    ) async throws {
        guard let pagination = ChatRequest.paginationDay(from: Date()) else {
            throw OutgoingChatRequestServiceError.unexpected("Invalid pagination day")
        }

        let topic1 = try ChatRequest.allPeerStatementsTopic(from: peer.accountId)
        let topic2 = try ChatRequest.paginationTopic(from: peer.accountId, day: pagination.day)
        let channel = try channelFactory.outgoingChannel(with: peer, ownKeyId: ownKeyId)

        let scaleEncodedPayload = try makePayload(message: message, to: peer, ownKeyId: ownKeyId)

        let builder = StatementSubmitParametersBuilder(
            signer: signer,
            logger: logger
        )
        .addTopic1(topic1)
        .addTopic2(topic2)
        .addTopic3(channel) // we add channel as topic2 also for easy discovery
        .addChannel(channel)
        .addExpiry(priorityFactory.makeTimestampPriority())
        .addScaleEncodedPayload(scaleEncodedPayload)

        try await statementStoreConnection.submitStatement(with: builder)
    }

    func encodedSize(
        of message: Chat.RequestMessage,
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> Int {
        try makePayload(message: message, to: peer, ownKeyId: ownKeyId).count
            + StatementSize.overhead(topicCount: Self.topicCount)
    }

    func isStored(
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own,
        signedBy accountId: AccountId
    ) async throws -> Bool {
        let channel = try channelFactory.outgoingChannel(with: peer, ownKeyId: ownKeyId).fixedStatementFieldData()
        let statements = try await statementStoreConnection.fetchStatements(with: .matchAll([channel]))

        return try statements.contains { data in
            let statement = try Statement(scaleDecoder: ScaleDecoder(data: data))

            guard case let .sr25519(_, signer) = statement.getProof() else { return false }

            return signer == accountId
        }
    }
}

private extension OutgoingChatRequestService {
    func makePayload(
        message: Chat.RequestMessage,
        to peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> Data {
        let remoteRequest = try requestFactory.createRemoteRequest(
            from: message,
            peerEncryptionPubKey: peer.publicKey,
            peerAccountId: peer.accountId,
            ownKeyId: ownKeyId
        )

        return try remoteRequest.scaleEncoded().scaleEncoded()
    }
}
