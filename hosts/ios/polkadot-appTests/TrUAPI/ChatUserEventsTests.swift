import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The user events that the host raises besides a posted message, such as a press on an `Actions` button. Each goes
/// out as a Chat Extension v2 event.
struct ChatUserEventsTests {
    private let productId: ProductId = "test.dot"

    @Test func aButtonPressReachesTheProductAsActionTriggered() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)

        await runtime.dispatchActionTrigger(roomId: "room", messageId: "msg-1", actionId: "yes")
        await runtime.dispose()

        let expected = PublishedChatEvent(
            roomId: "room",
            peer: "native",
            event: .actionTriggered(ActionTrigger(messageId: "msg-1", actionId: "yes", payload: nil))
        )
        #expect(execution.publishedChatEvents == [expected])
    }

    /// A message that the product pushed with `update_feed` has a store id that starts with its room. The
    /// product gets its own id back.
    @Test func aButtonPressNamesThePushedMessageByTheIdOfTheProduct() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)
        let storedId = Chat.ProductMessageId.stored("m1", in: .chatExtension(productId, roomId: "room"))

        await runtime.dispatchActionTrigger(roomId: "room", messageId: storedId, actionId: "yes")
        await runtime.dispose()

        #expect(execution.publishedChatEvents.map(\.event) == [
            .actionTriggered(ActionTrigger(messageId: "m1", actionId: "yes", payload: nil))
        ])
    }

    /// A press in a chat with no room cannot be addressed, so nothing reaches the product.
    @Test func aButtonPressWithoutARoomIsDropped() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)

        await runtime.dispatchActionTrigger(roomId: nil, messageId: "msg-1", actionId: "yes")
        await runtime.dispose()

        #expect(execution.publishedChatEvents.isEmpty)
    }

    @Test(arguments: [
        (
            ProductChatUserEvent.messagePosted(messageId: "u1", text: "hi", replyTo: "m1"),
            ChatEvent.messagePosted(messageId: "u1", body: .text(text: "hi"), replyTo: "m1")
        ),
        (.edited(messageId: "u1", text: "hey"), .userEdited(messageId: "u1", body: .text(text: "hey"))),
        (.deleted(messageId: "m1"), .userDeleted(messageId: "m1")),
        (.reacted(messageId: "m1", emoji: "👍", placed: true), .userReacted(messageId: "m1", emoji: "👍", placed: true)),
        (
            .reacted(messageId: "m1", emoji: "👍", placed: false),
            .userReacted(messageId: "m1", emoji: "👍", placed: false)
        ),
        (.seen(messageId: "m1"), .seen(messageId: "m1"))
    ])
    func eachUserEventGoesOutAsItsChatEvent(event: ProductChatUserEvent, expected: ChatEvent) async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)

        try await runtime.publishUserEvent(event, roomId: "room")
        await runtime.dispose()

        #expect(execution.publishedChatEvents == [PublishedChatEvent(roomId: "room", peer: "native", event: expected)])
    }

    @Test func aUserEventWithoutARoomFails() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)

        await #expect(throws: TrUAPIChatHandler.ChatSeamError.roomlessChat) {
            try await runtime.publishUserEvent(.deleted(messageId: "m1"), roomId: nil)
        }
        await runtime.dispose()
        #expect(execution.publishedChatEvents.isEmpty)
    }

    /// The native runtime keeps the first version: a posted text goes to the worker, every other event stays.
    @Test func theNativeRuntimeSendsOnlyThePostedText() async throws {
        let runtime = ManagedChatRuntime(productId: productId, manager: InertWorkerManager())

        try await runtime.publishUserEvent(.edited(messageId: "u1", text: "x"), roomId: "room")
        try await runtime.publishUserEvent(.seen(messageId: "u1"), roomId: "room")
        await #expect(throws: ManagedChatRuntimeError.self) {
            try await runtime.publishUserEvent(
                .messagePosted(messageId: "u1", text: "hi", replyTo: nil),
                roomId: "room"
            )
        }
    }
}

private extension ChatUserEventsTests {
    func startedRuntime(on execution: MockProductExecution) async throws -> TrUAPIChatHandler {
        let workers = StubWorkerManager()
        workers.publish([productId: execution])
        let runtime = TrUAPIChatHandler(productId: productId, workers: workers)
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        return runtime
    }
}

private struct InertWorkerManager: ProductWorkerManaging {
    func acquire(productId _: ProductId) async -> ProductWorkerLease {
        ProductWorkerLease(token: ProductWorkerToken {}, result: .failure(CancellationError()))
    }
}
