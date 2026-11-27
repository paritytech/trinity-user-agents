import Foundation
import Testing

@testable import polkadot_app

struct SeenReceiptDeliveryRecorderTests {
    private let fixture = SeenReceiptStorageFixture()

    @Test func deliveredSeenStoresOrderAndTimestampOfIncomingMessage() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", order: 3, timestamp: 50)

        try await makeRecorder().recordDelivered(
            [seenContent(messageId: "last-seen", timestamp: 50)],
            to: fixture.peerAccountId
        )

        let lastSentMark = try await fixture.lastSentSeenMark()
        #expect(lastSentMark == (3, 50))
    }

    @Test func olderDeliveredAfterNewerKeepsNewerMark() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "newer", order: 3, timestamp: 50)
        try await insertMessage(id: "older", order: 1, timestamp: 10)
        let recorder = makeRecorder()

        try await recorder.recordDelivered([seenContent(messageId: "newer", timestamp: 50)], to: fixture.peerAccountId)
        try await recorder.recordDelivered([seenContent(messageId: "older", timestamp: 10)], to: fixture.peerAccountId)

        let lastSentMark = try await fixture.lastSentSeenMark()
        #expect(lastSentMark == (3, 50))
    }

    @Test func unknownMessageIdLeavesMarkUnchanged() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "known", order: 3, timestamp: 50)

        try await makeRecorder().recordDelivered(
            [seenContent(messageId: "unknown", timestamp: 99)],
            to: fixture.peerAccountId
        )

        let lastSentMark = try await fixture.lastSentSeenMark()
        #expect(lastSentMark == (0, 0))
    }
}

private extension SeenReceiptDeliveryRecorderTests {
    func makeRecorder() -> SeenReceiptDeliveryRecorder {
        SeenReceiptDeliveryRecorder(storageFacade: fixture.facade)
    }

    func insertMessage(id: Chat.MessageId, order: Int64, timestamp: Chat.Timestamp) async throws {
        try await fixture.insertMessage(
            fixture.makeMessage(id: id, status: .incoming(.seen), timestamp: timestamp),
            order: order
        )
    }

    func seenContent(
        messageId: Chat.MessageId,
        timestamp: Chat.Timestamp
    ) -> ChatRemoteMessageContent.SeenContent {
        .init(messageId: messageId, timestamp: timestamp)
    }
}
