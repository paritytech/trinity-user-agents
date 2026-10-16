import Foundation
import Operation_iOS
import AsyncExtensions

protocol ChatExtensionActionContextProtocol {
    /// The chat of the screen where the action happened. A message of a product-source room is not in the store, so
    /// the room of the action comes from here (VERBOSITY-WASM-PRODUCT C15).
    var chatId: Chat.Id? { get }

    func getMessage(messageId: Chat.MessageId) async throws -> Chat.LocalMessage?
}

extension ChatExtensionActionContextProtocol {
    var chatId: Chat.Id? {
        nil
    }
}

final class ChatExtensionActionContext {
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let chatId: Chat.Id?

    init(messageRepository: AnyDataProviderRepository<Chat.LocalMessage>, chatId: Chat.Id? = nil) {
        self.messageRepository = messageRepository
        self.chatId = chatId
    }
}

extension ChatExtensionActionContext: ChatExtensionActionContextProtocol {
    func getMessage(messageId: Chat.MessageId) async throws -> Chat.LocalMessage? {
        try await messageRepository.fetchOperation(
            by: { messageId },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()
    }
}
