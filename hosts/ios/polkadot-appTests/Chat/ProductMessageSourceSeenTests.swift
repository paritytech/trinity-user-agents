import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A `Seen` that the product does not take goes again, so the unread count of the product does not stay after the
/// user read the room.
struct ProductMessageSourceSeenTests {
    private let productId = "chat.dot"
    private let roomId = "dm-1"

    @Test func aSeenThatTheProductRefusesGoesAgain() async throws {
        let failures = OSAllocatedUnfairLock(initialState: 2)
        let querying = ScriptedProductChatQuerying(
            page: { _ in .init(messages: (1 ... 3).map { message($0) }, next: nil) },
            action: { _ in
                let fail = failures.withLock { left in
                    defer { left -= 1 }
                    return left > 0
                }
                if fail {
                    throw ChatQueryError.actionRefused(.rejected(reason: "Verbosity is starting or busy. Try again."))
                }
                return .acknowledged
            }
        )
        let source = makeSource(querying)
        await source.start()
        await source.waitForFirstPage()

        _ = try await source.perform(.markSeen(["m3"]))
        await waitUntil { await source.reportedSeenMessageId == "m3" }

        #expect(querying.actionRequests.map(\.action) == Array(repeating: .seen(messageId: "m3"), count: 3))
    }

    @Test func aSeenStopsAfterItsAttemptsAndGoesWithTheNextMark() async throws {
        let querying = ScriptedProductChatQuerying(
            page: { _ in .init(messages: (1 ... 3).map { message($0) }, next: nil) },
            action: { _ in throw ChatQueryError.timedOut }
        )
        let source = makeSource(querying)
        await source.start()
        await source.waitForFirstPage()

        _ = try await source.perform(.markSeen(["m3"]))
        await waitUntil { querying.actionRequests.count == 3 }
        try await Task.sleep(for: .milliseconds(50))
        #expect(querying.actionRequests.count == 3)
        #expect(await source.reportedSeenMessageId == nil)

        _ = try await source.perform(.markSeen(["m3"]))
        await waitUntil { querying.actionRequests.count == 4 }
        #expect(querying.actionRequests.count >= 4)
    }

    /// The first page of a room keeps the messages of the contact that the product gives as `Delivered` new, so
    /// the screen draws the "Unread messages" separator above the first one, as the native chat does above the
    /// welcome message of an incoming request.
    @Test func theFirstPageKeepsUnseenMessagesNew() async throws {
        let querying = ScriptedProductChatQuerying(
            page: { _ in .init(messages: (1 ... 2).map { message($0) }, next: nil) }
        )
        let source = makeSource(querying)
        await source.start()
        await source.waitForFirstPage()

        let snapshot = await source.snapshot
        #expect(snapshot.messages.map(\.status) == [.incoming(.new), .incoming(.new)])

        let model = await ChatInteractorState().onSnapshot(snapshot)
        #expect(model.initiallyUnreadMessage == "m1")
        #expect(querying.actionRequests.isEmpty)
    }
}

private extension ProductMessageSourceSeenTests {
    func makeSource(_ querying: ScriptedProductChatQuerying) -> ProductMessageSource {
        ProductMessageSource(
            chatId: .chatExtension(productId, roomId: roomId),
            productId: productId,
            roomId: roomId,
            querying: querying,
            cache: ProductMessageCache(),
            settings: .init(
                retryDelay: .milliseconds(1),
                reportsSeen: true,
                seenRetryDelay: .milliseconds(5),
                seenAttempts: 3
            ),
            logger: StubLogger()
        )
    }

    func waitUntil(_ condition: @Sendable () async -> Bool) async {
        for _ in 0 ..< 1_000 where await !condition() {
            try? await Task.sleep(for: .milliseconds(2))
        }
    }
}

private func message(_ index: Int) -> ChatMessage {
    ChatMessage(
        messageId: "m\(index)",
        origin: ChatMessageOrigin(participantId: "c1"),
        timestamp: UInt64(index) * 1_000,
        body: .text(text: "message \(index)"),
        status: .delivered,
        replyTo: nil,
        replyPreview: nil,
        editedAt: nil,
        reactions: [],
        request: false
    )
}
