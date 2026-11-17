import Foundation
import StructuredConcurrencyTestSupport
import Testing

@testable import polkadot_app

struct SeenReceiptServiceTests {
    private let fixture = SeenReceiptStorageFixture()
    private let sender = SpyDirectMessageSender()
    private let clock = ManualClock()

    @Test func sendsGreatestOrderSeenDisplayableIncomingMessage() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "first", status: .incoming(.seen), order: 1, timestamp: 100)
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 3, timestamp: 50)
        try await insertMessage(id: "middle", status: .incoming(.seen), order: 2, timestamp: 500)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sentContent() == .seen(.init(messageId: "last-seen", timestamp: 50)))
        #expect(sender.sent.first?.accountId == fixture.peerAccountId)
    }

    @Test func nonDisplayableSeenMessagesDoNotAdvanceLastSeenMessage() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)
        try await insertMessage(
            id: "reaction",
            status: .incoming(.seen),
            order: 2,
            timestamp: 20,
            content: .reacted(.init(messageId: "last-seen", emoji: "👍"))
        )
        try await insertMessage(
            id: "edit",
            status: .incoming(.seen),
            order: 3,
            timestamp: 30,
            content: .edited(.init(messageId: "last-seen", newContent: .init(text: "edited", attachments: nil)))
        )
        try await insertMessage(
            id: "compacted",
            status: .incoming(.seen),
            order: 4,
            timestamp: 40,
            content: .compactedMessages(.init(
                claimIdentifier: Data(repeating: 0x01, count: 32),
                claimTicket: Data(repeating: 0x02, count: 32),
                node: .wssUrl("wss://localhost:8000")
            ))
        )
        try await insertMessage(
            id: "accepted",
            status: .incoming(.seen),
            order: 5,
            timestamp: 50,
            content: .chatAccepted(.init(messageId: "last-seen"))
        )
        try await insertMessage(
            id: "device",
            status: .incoming(.seen),
            order: 6,
            timestamp: 60,
            content: .deviceAdded(.init(
                statementAccountId: Data(repeating: 0x01, count: 32),
                encryptionPublicKey: Data(repeating: 0x02, count: 32)
            ))
        )
        try await insertMessage(
            id: "contactAdded",
            status: .incoming(.seen),
            order: 7,
            timestamp: 70,
            content: .contactAdded
        )
        try await insertMessage(
            id: "leftChat",
            status: .incoming(.seen),
            order: 8,
            timestamp: 80,
            content: .leftChat
        )
        try await insertMessage(
            id: "multiChatAccepted",
            status: .incoming(.seen),
            order: 9,
            timestamp: 90,
            content: .multiChatAccepted(.init(
                requestId: "request",
                device: .init(
                    statementAccountId: Data(repeating: 0x01, count: 32),
                    encryptionPublicKey: Data(repeating: 0x02, count: 32)
                )
            ))
        )

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sentContent() == .seen(.init(messageId: "last-seen", timestamp: 10)))
    }

    @Test func equalOrderFallsBackToLaterTimestamp() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "older", status: .incoming(.seen), order: 2, timestamp: 100)
        try await insertMessage(id: "newer", status: .incoming(.seen), order: 2, timestamp: 200)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sentContent() == .seen(.init(messageId: "newer", timestamp: 200)))
    }

    @Test func ignoresUnseenAndOutgoingMessages() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)
        try await insertMessage(id: "unseen", status: .incoming(.new), order: 5, timestamp: 90)
        try await insertMessage(id: "outgoing", status: .outgoing(.sent), order: 9, timestamp: 95)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sentContent() == .seen(.init(messageId: "last-seen", timestamp: 10)))
    }

    @Test func extensionChatSendsNothing() async throws {
        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: .chatExtension("extension", roomId: nil)))

        #expect(sender.sent.isEmpty)
    }

    @Test func chatWithoutCandidateSendsNothing() async throws {
        try await fixture.seedChat()

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sender.sent.isEmpty)
    }

    @Test func pendingChatRequestSendsNothing() async throws {
        let request = Chat.Request(
            requestId: "request",
            contactAccountId: fixture.peerAccountId,
            timestamp: 1,
            status: .outgoing(.pending),
            message: nil
        )
        try await fixture.seedChat(pendingRequest: request)
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(try await fixture.hasPendingRequest())
        #expect(sender.sent.isEmpty)
    }

    @Test func blockedContactSendsNothing() async throws {
        try await fixture.seedChat(isBlocked: true)
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sender.sent.isEmpty)
    }

    @Test func lastSeenEqualToLastSentMarkDoesNotSend() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 2, timestamp: 20)
        try await fixture.seedLastSentSeen(order: 2, timestamp: 20)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sender.sent.isEmpty)
    }

    @Test func sameOrderNewerTimestampSends() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "newer", status: .incoming(.seen), order: 2, timestamp: 200)
        try await fixture.seedLastSentSeen(order: 2, timestamp: 100)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sentContent() == .seen(.init(messageId: "newer", timestamp: 200)))
    }

    @Test func higherOrderSendsEvenWithOlderTimestamp() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "higher", status: .incoming(.seen), order: 3, timestamp: 50)
        try await fixture.seedLastSentSeen(order: 2, timestamp: 100)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sentContent() == .seen(.init(messageId: "higher", timestamp: 50)))
    }

    @Test func burstInsideWindowSendsLeadingThenNewestAtWindowEnd() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "older", status: .incoming(.seen), order: 1, timestamp: 100)

        let service = makeService()
        let task = service.scheduleSeenReceipt(for: fixture.chatId)
        await clock.waitForSleep(for: Self.throttleInterval)

        try await insertMessage(id: "newest", status: .incoming(.seen), order: 2, timestamp: 200)
        service.scheduleSeenReceipt(for: fixture.chatId)
        service.scheduleSeenReceipt(for: fixture.chatId)

        await clock.resumeSleep(for: Self.throttleInterval)
        await clock.resumeSleep(for: Self.throttleInterval)
        await task.value

        #expect(sentContents() == [
            .seen(.init(messageId: "older", timestamp: 100)),
            .seen(.init(messageId: "newest", timestamp: 200))
        ])
    }

    @Test func firstTriggerSendsBeforeWindowEnds() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)

        let task = makeService().scheduleSeenReceipt(for: fixture.chatId)
        await clock.waitForSleep(for: Self.throttleInterval)

        #expect(sender.sent.count == 1)

        await clock.resumeSleep(for: Self.throttleInterval)
        await task.value
    }

    @Test func pendingTriggerWithoutAdvanceDoesNotResend() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)

        let service = makeService()
        let task = service.scheduleSeenReceipt(for: fixture.chatId)
        service.scheduleSeenReceipt(for: fixture.chatId)

        await clock.resumeSleep(for: Self.throttleInterval)
        await clock.resumeSleep(for: Self.throttleInterval)
        await task.value

        #expect(sender.sent.count == 1)
    }

    @Test func continuousTriggersSendOncePerWindow() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "first", status: .incoming(.seen), order: 1, timestamp: 100)

        let service = makeService()
        let task = service.scheduleSeenReceipt(for: fixture.chatId)
        await clock.waitForSleep(for: Self.throttleInterval)

        try await insertMessage(id: "second", status: .incoming(.seen), order: 2, timestamp: 200)
        service.scheduleSeenReceipt(for: fixture.chatId)
        await clock.resumeSleep(for: Self.throttleInterval)
        await clock.waitForSleep(for: Self.throttleInterval)

        try await insertMessage(id: "third", status: .incoming(.seen), order: 3, timestamp: 300)
        service.scheduleSeenReceipt(for: fixture.chatId)
        await clock.resumeSleep(for: Self.throttleInterval)

        await clock.resumeSleep(for: Self.throttleInterval)
        await task.value

        #expect(sentContents() == [
            .seen(.init(messageId: "first", timestamp: 100)),
            .seen(.init(messageId: "second", timestamp: 200)),
            .seen(.init(messageId: "third", timestamp: 300))
        ])
    }

    @Test func sendingDoesNotPersistLastSentMark() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)

        await runScheduledSeenReceipt(makeService().scheduleSeenReceipt(for: fixture.chatId))

        #expect(sender.sent.count == 1)
        let lastSentMark = try await fixture.lastSentSeenMark()
        #expect(lastSentMark == (0, 0))
    }

    @Test func repeatedTriggerResendsWhileMarkIsNotAdvancedByDelivery() async throws {
        try await fixture.seedChat()
        try await insertMessage(id: "last-seen", status: .incoming(.seen), order: 1, timestamp: 10)
        let service = makeService()

        await runScheduledSeenReceipt(service.scheduleSeenReceipt(for: fixture.chatId))
        await runScheduledSeenReceipt(service.scheduleSeenReceipt(for: fixture.chatId))

        #expect(sender.sent.count == 2)
    }
}

private extension SeenReceiptServiceTests {
    static let throttleInterval: Duration = .seconds(1)

    func makeService() -> SeenReceiptService {
        SeenReceiptService(storageFacade: fixture.facade, sender: sender, clock: clock)
    }

    func runScheduledSeenReceipt(_ task: Task<Void, Never>) async {
        await clock.resumeSleep(for: Self.throttleInterval)
        await task.value
    }

    func insertMessage(
        id: Chat.MessageId,
        status: Chat.LocalMessage.Status,
        order: Int64,
        timestamp: Chat.Timestamp,
        content: Chat.LocalMessage.Content = .text("text")
    ) async throws {
        try await fixture.insertMessage(
            fixture.makeMessage(id: id, status: status, timestamp: timestamp, content: content),
            order: order
        )
    }

    func sentContent() -> Chat.RemoteMessageContentV1.MessageContent? {
        sender.sent.first?.message.versioned.ensureV1()?.content
    }

    func sentContents() -> [Chat.RemoteMessageContentV1.MessageContent] {
        sender.sent.compactMap { $0.message.versioned.ensureV1()?.content }
    }
}
