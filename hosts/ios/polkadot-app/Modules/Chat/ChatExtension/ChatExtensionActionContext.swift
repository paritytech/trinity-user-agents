import Foundation
import Operation_iOS
import AsyncExtensions

protocol ChatExtensionActionContextProtocol {
    func getMessage(messageId: Chat.MessageId) async throws -> Chat.LocalMessage?
}

final class ChatExtensionActionContext {
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>

    init(messageRepository: AnyDataProviderRepository<Chat.LocalMessage>) {
        self.messageRepository = messageRepository
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
