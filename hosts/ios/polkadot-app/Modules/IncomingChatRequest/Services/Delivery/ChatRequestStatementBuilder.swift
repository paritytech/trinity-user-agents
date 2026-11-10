import Foundation
import MessageExchangeKit
import StatementDelivery
import StatementStore

enum ChatRequestStatementBuilderError: Error {
    case invalidPaginationDay
}

/// The request statement: discovery topics, the peer channel and the encrypted request with its inner proof.
struct ChatRequestStatementBuilder: @unchecked Sendable {
    private static let topicCount = 3

    let requestFactory: ChatRequestFactoryProtocol
    let channelFactory: ChatRequestChannelFactoryProtocol
    let priorityFactory: StatementPriorityMaking
}

extension ChatRequestStatementBuilder: OutgoingStatementBuilding {
    func makeUnsignedStatement(for input: OutgoingChatRequestInput) throws -> UnsignedStatement {
        guard let pagination = ChatRequest.paginationDay(from: Date()) else {
            throw ChatRequestStatementBuilderError.invalidPaginationDay
        }

        let peer = input.session.peer
        let channel = try channelFactory.outgoingChannel(with: peer, ownKeyId: input.session.own)

        return try UnsignedStatement()
            .addTopic1(ChatRequest.allPeerStatementsTopic(from: peer.accountId))
            .addTopic2(ChatRequest.paginationTopic(from: peer.accountId, day: pagination.day))
            .addTopic3(channel) // the channel doubles as a topic for discovery
            .addChannel(channel)
            .addExpiry(priorityFactory.makeTimestampPriority())
            .addScaleEncodedPayload(makePayload(for: input))
    }

    func encodedSize(of input: OutgoingChatRequestInput) throws -> Int {
        try makePayload(for: input).count + StatementSize.overhead(topicCount: Self.topicCount)
    }

    func storageTopicFilter(for input: OutgoingChatRequestInput) throws -> TopicFilter {
        let channel = try channelFactory.outgoingChannel(with: input.session.peer, ownKeyId: input.session.own)
        return try .matchAll([channel.fixedStatementFieldData()])
    }
}

private extension ChatRequestStatementBuilder {
    func makePayload(for input: OutgoingChatRequestInput) throws -> Data {
        let peer = input.session.peer
        let remoteRequest = try requestFactory.createRemoteRequest(
            from: input.message,
            peerEncryptionPubKey: peer.publicKey,
            peerAccountId: peer.accountId,
            ownSignKeyId: input.session.own.signKeyId
        )

        return try remoteRequest.scaleEncoded().scaleEncoded()
    }
}
