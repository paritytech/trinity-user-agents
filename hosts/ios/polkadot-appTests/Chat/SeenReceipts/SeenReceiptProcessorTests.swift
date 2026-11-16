import Foundation
import Testing

@testable import polkadot_app

struct SeenReceiptProcessorTests {
    private let fixture = SeenReceiptStorageFixture()

    @Test func advancesPeerSeenOrderToKnownOutgoingMessageOrder() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "sent", order: 4)

        try await makeProcessor().apply([seenContent("sent")], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 4)
    }

    @Test func olderSeenReceiptKeepsNewerPeerSeenOrder() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "older", order: 4)
        try await insertOutgoing(id: "newer", order: 9)
        let processor = makeProcessor()

        try await processor.apply([seenContent("newer")], from: fixture.peerAccountId)
        try await processor.apply([seenContent("older")], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 9)
    }

    @Test func unknownMessageIdResolvesToOutgoingOrderAtOrBeforeTimestamp() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "first", order: 1, timestamp: 100)
        try await insertOutgoing(id: "second", order: 2, timestamp: 200)
        try await insertOutgoing(id: "third", order: 3, timestamp: 300)

        try await makeProcessor().apply([seenContent("unknown", timestamp: 250)], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 2)
    }

    @Test func unknownIdWithoutOutgoingRowAtOrBeforeTimestampLeavesPeerSeenOrderUnchanged() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "sent", order: 4, timestamp: 1_000)

        try await makeProcessor().apply([seenContent("unknown", timestamp: 500)], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 0)
    }

    @Test func timestampFallbackNeverLowersPeerSeenOrder() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "early", order: 1, timestamp: 100)
        try await insertOutgoing(id: "known", order: 3, timestamp: 1_000)
        let processor = makeProcessor()

        try await processor.apply([seenContent("known")], from: fixture.peerAccountId)
        try await processor.apply([seenContent("unknown", timestamp: 150)], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 3)
    }

    @Test func timestampFallbackIgnoresIncomingRows() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "sent", order: 2, timestamp: 200)
        try await fixture.insertMessage(
            fixture.makeMessage(id: "incoming", status: .incoming(.new), timestamp: 100),
            order: 5
        )

        try await makeProcessor().apply([seenContent("unknown", timestamp: 150)], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 0)
    }

    @Test func timestampFallbackSkipsNewAndFailedOutgoingRows() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "sent", order: 1, timestamp: 100)
        try await insertOutgoing(id: "pending", order: 2, timestamp: 200, status: .outgoing(.new))
        try await insertOutgoing(id: "failed", order: 3, timestamp: 210, status: .outgoing(.failed))

        try await makeProcessor().apply([seenContent("unknown", timestamp: 250)], from: fixture.peerAccountId)

        #expect(try await fixture.peerSeenOrder() == 1)
    }

    @Test func neverCreatesMessageRows() async throws {
        try await fixture.seedChat()
        try await insertOutgoing(id: "sent", order: 4)
        let processor = makeProcessor()

        try await processor.apply([seenContent("unknown")], from: fixture.peerAccountId)
        try await processor.apply([seenContent("sent")], from: fixture.peerAccountId)

        #expect(try await fixture.messageCount() == 1)
    }

    @Test func seenReceiptIsExtracted() async throws {
        let seenContents = makeProcessor().extractSeenReceipts(from: [receipt("sent")])

        #expect(seenContents.map(\.messageId) == ["sent"])
    }
}

private extension SeenReceiptProcessorTests {
    func makeProcessor() -> SeenReceiptProcessor {
        SeenReceiptProcessor(storageFacade: fixture.facade)
    }

    func insertOutgoing(
        id: Chat.MessageId,
        order: Int64,
        timestamp: Chat.Timestamp = 1_000,
        status: Chat.LocalMessage.Status = .outgoing(.sent)
    ) async throws {
        try await fixture.insertMessage(
            fixture.makeMessage(id: id, status: status, timestamp: timestamp),
            order: order
        )
    }

    func seenContent(
        _ messageId: Chat.MessageId,
        timestamp: UInt64 = 1_000
    ) -> ChatRemoteMessageContent.SeenContent {
        .init(messageId: messageId, timestamp: timestamp)
    }

    func receipt(_ messageId: Chat.MessageId) -> Chat.RemoteMessage {
        Chat.RemoteMessage.newMessage(with: ChatRemoteMessageContent.seen(seenContent(messageId)))
    }
}
