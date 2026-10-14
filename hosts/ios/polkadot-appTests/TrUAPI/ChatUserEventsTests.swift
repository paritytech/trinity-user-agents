import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The two user events that the host raises besides a posted message: a press on an `Actions` button and a
/// slash command.
struct ChatUserEventsTests {
    private let productId: ProductId = "test.dot"

    @Test func aButtonPressReachesTheProductAsActionTriggered() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)

        await runtime.dispatchActionTrigger(roomId: "room", messageId: "msg-1", actionId: "yes")
        await runtime.dispose()

        let expected = HostChatActionSubscribeItem(
            roomId: "room",
            peer: "native",
            payload: .actionTriggered(ActionTrigger(messageId: "msg-1", actionId: "yes", payload: nil))
        )
        #expect(execution.publishedChatActions == [expected])
    }

    /// A press in a chat with no room cannot be addressed, so nothing reaches the product.
    @Test func aButtonPressWithoutARoomIsDropped() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)

        await runtime.dispatchActionTrigger(roomId: nil, messageId: "msg-1", actionId: "yes")
        await runtime.dispose()

        #expect(execution.publishedChatActions.isEmpty)
    }

    @Test func aCommandReachesTheProductAsCommand() async throws {
        let execution = MockProductExecution()
        let runtime = try await startedRuntime(on: execution)
        let command = try #require(Chat.Command(text: "/roll 2d6 fast"))

        let taken = await runtime.dispatchCommand(command, roomId: "room")
        await runtime.dispose()

        #expect(taken)
        let expected = HostChatActionSubscribeItem(
            roomId: "room",
            peer: "native",
            payload: .command(ChatCommand(command: "roll", payload: "2d6 fast"))
        )
        #expect(execution.publishedChatActions == [expected])
    }

    /// The host posts the text instead when the product cannot take the command yet.
    @Test func aCommandBeforeTheWorkerIsUpIsNotTaken() async throws {
        let runtime = TrUAPIChatHandler(productId: productId, workers: StubWorkerManager())
        let command = try #require(Chat.Command(text: "/help"))

        #expect(await runtime.dispatchCommand(command, roomId: "room") == false)
        await runtime.dispose()
    }

    /// The native runtime has no command event, so the text stays a message.
    @Test func theNativeRuntimeDoesNotTakeCommands() async throws {
        let runtime = ManagedChatRuntime(productId: productId, manager: InertWorkerManager())
        let command = try #require(Chat.Command(text: "/help"))

        #expect(await runtime.dispatchCommand(command, roomId: "room") == false)
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
