import AsyncExtensions
import Foundation
@testable import polkadot_app

/// A registry with one chat bot and no widgets.
struct SingleBotRegistry: ChatExtensionsRegistering {
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
