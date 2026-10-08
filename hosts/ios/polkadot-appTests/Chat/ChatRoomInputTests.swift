import AsyncExtensions
import Foundation
import Testing
@testable import polkadot_app

struct ChatRoomInputTests {
    private let bot = ChatSampleExtension(logger: StubLogger())

    private func room(hidesTextInput: Bool) -> Chat.LocalModel {
        Chat.LocalModel.newChatWithRoom(
            extensionId: bot.identifier,
            roomId: "room",
            roomMetadata: Chat.RoomMetadata(
                chatRelativeId: "room",
                name: nil,
                icon: nil,
                hidesTextInput: hidesTextInput
            )
        )
    }

    @Test func aRoomThatHidesItsInputShowsNone() {
        let registry = SingleBotRegistry(bot: bot)

        #expect(room(hidesTextInput: true).peerMetadata(using: registry).input == .empty)
    }

    @Test func aRoomThatKeepsItsInputShowsTheBotInput() {
        let registry = SingleBotRegistry(bot: bot)

        #expect(room(hidesTextInput: false).peerMetadata(using: registry).input == bot.peerMetadata.input)
    }
}

private struct SingleBotRegistry: ChatExtensionsRegistering {
    let bot: ChatExtensionBotProtocol

    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> {
        AsyncStream<ChatExtensionRegistryChange> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func discover() {}

    func getExtensions(for _: Chat.Id) -> [ChatExtending] { [bot] }

    func getChatExtensionBot(for extensionId: ChatExtension.Id) -> ChatExtensionBotProtocol? {
        extensionId == bot.identifier ? bot : nil
    }

    func getWidgetProviders() -> [ChatExtensionWidgetProvider] { [] }
}
