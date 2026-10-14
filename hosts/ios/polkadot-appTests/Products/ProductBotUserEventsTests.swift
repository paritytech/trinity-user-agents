import Foundation
import os
import PolkadotUI
import Products
import Testing
import UIKitExt
@testable import polkadot_app

/// The product bot hands a button press and a command to its runtime with the room of the message.
struct ProductBotUserEventsTests {
    private let runtime = RecordingUserEventsRuntime()

    @Test func aButtonPressGoesToTheRuntimeWithTheRoomOfItsMessage() async {
        let bot = makeBot()
        let message = Chat.LocalMessage.newMessage(
            to: .chatExtension("dim2.paseo", roomId: "lobby"),
            content: .text("pick"),
            messageId: "msg-1"
        )

        await bot.process(
            action: .productButton(messageId: "msg-1", actionId: "yes"),
            context: SingleMessageContext(message: message)
        )

        #expect(runtime.triggers == [.init(roomId: "lobby", messageId: "msg-1", actionId: "yes")])
    }

    @Test func aCommandGoesToTheRuntime() async {
        let bot = makeBot()
        runtime.takesCommands = true

        let taken = await bot.handle(command: Chat.Command(name: "help", arguments: ""), roomId: "lobby")

        #expect(taken)
        #expect(runtime.commands == [.init(name: "help", arguments: "")])
    }

    private func makeBot() -> ProductBot {
        ProductBot(
            product: Product(id: "dim2.paseo", name: "Jollity"),
            description: nil,
            runtime: runtime,
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }
}

private struct SingleMessageContext: ChatExtensionActionContextProtocol {
    let message: Chat.LocalMessage

    func getMessage(messageId: Chat.MessageId) async throws -> Chat.LocalMessage? {
        message.messageId == messageId ? message : nil
    }
}

private final class RecordingUserEventsRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    struct Trigger: Equatable {
        let roomId: String?
        let messageId: String
        let actionId: String
    }

    private struct State {
        var triggers: [Trigger] = []
        var commands: [Chat.Command] = []
        var takesCommands = false
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var triggers: [Trigger] { state.withLock { $0.triggers } }
    var commands: [Chat.Command] { state.withLock { $0.commands } }
    var takesCommands: Bool {
        get { state.withLock { $0.takesCommands } }
        set { state.withLock { $0.takesCommands = newValue } }
    }

    func dispatchActionTrigger(roomId: String?, messageId: String, actionId: String) async {
        state.withLock { $0.triggers.append(Trigger(roomId: roomId, messageId: messageId, actionId: actionId)) }
    }

    func dispatchCommand(_ command: Chat.Command, roomId _: String?) async -> Bool {
        state.withLock { state in
            state.commands.append(command)
            return state.takesCommands
        }
    }

    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}
    func onUserMessage(text _: String, roomId _: String?) async throws {}

    func renderMessage(
        roomId _: String?,
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(
        roomId _: String?,
        messageId _: String,
        messageType _: String?,
        actionId _: String,
        payload _: String?
    ) async {}

    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
    func dispose() async {}
}
