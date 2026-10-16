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

    /// A product-source room keeps no message in the store: the press names the room of the screen.
    @Test func aButtonPressOfAProductSourceRoomGoesWithTheRoomOfTheScreen() async {
        let bot = makeBot()

        await bot.process(
            action: .productButton(messageId: "n1", actionId: "mark-verified"),
            context: ScreenContext(chatId: .chatExtension("dim2.paseo", roomId: "dm-2"))
        )

        #expect(runtime.triggers == [.init(roomId: "dm-2", messageId: "n1", actionId: "mark-verified")])
    }

    @Test func aDeclaredCommandGoesToTheRuntime() async {
        let bot = makeBot(commands: ["help"])
        runtime.takesCommands = true

        let taken = await bot.handle(command: Chat.Command(name: "help", arguments: ""), roomId: "lobby")

        #expect(taken)
        #expect(runtime.commands == [.init(name: "help", arguments: "")])
    }

    /// 02-DESIGN §13 item 12: any other `/text` is a message, so the host posts the text.
    @Test func aCommandThatTheRoomDoesNotDeclareIsAMessage() async {
        let bot = makeBot(commands: ["help"])
        runtime.takesCommands = true

        #expect(await bot.handle(command: Chat.Command(name: "roll", arguments: "2d6"), roomId: "lobby") == false)
        #expect(await makeBot(commands: nil).handle(
            command: Chat.Command(name: "help", arguments: ""),
            roomId: "lobby"
        ) == false)
        #expect(await bot.handle(command: Chat.Command(name: "help", arguments: ""), roomId: nil) == false)
        #expect(runtime.commands.isEmpty)
    }

    private func makeBot(commands: [String]? = nil) -> ProductBot {
        let declarations = Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: nil,
            configGeneration: 0,
            preview: .hostDefault,
            toolbar: nil,
            suggestions: nil,
            commands: commands?.map { Chat.RoomCommand(name: $0, description: "", argumentHint: nil) },
            footerConfig: nil
        )
        return ProductBot(
            product: Product(id: "dim2.paseo", name: "Jollity"),
            description: nil,
            runtime: runtime,
            resolveImage: WidgetImageResolver { _ in nil },
            rooms: StubProductRooms(declarations: declarations)
        )
    }
}

private struct ScreenContext: ChatExtensionActionContextProtocol {
    let chatId: Chat.Id?

    func getMessage(messageId _: Chat.MessageId) async throws -> Chat.LocalMessage? {
        nil
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
