import Foundation
import Operation_iOS
import SubstrateSdk

/// Outgoing messages the user wrote on this device that are still waiting to be sent to a contact.
protocol ChatPendingMessagesServicing: Sendable {
    func hasPendingMessages(to contactAccountId: AccountId) async throws -> Bool
    func failPendingMessages(to contactAccountId: AccountId) async throws
}

final class ChatPendingMessagesService: @unchecked Sendable {
    private let repositoryFactory: ChatMessageStatusUpdateRepositoryMaking

    init(repositoryFactory: ChatMessageStatusUpdateRepositoryMaking = ChatMessageStatusUpdateRepositoryFactory()) {
        self.repositoryFactory = repositoryFactory
    }
}

extension ChatPendingMessagesService: ChatPendingMessagesServicing {
    func hasPendingMessages(to contactAccountId: AccountId) async throws -> Bool {
        try await !pendingRepository(for: contactAccountId)
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .isEmpty
    }

    func failPendingMessages(to contactAccountId: AccountId) async throws {
        let repository = pendingRepository(for: contactAccountId)
        let pending = try await repository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()

        guard !pending.isEmpty else { return }

        let failed = pending.map { $0.replacingStatus(.outgoing(.failed)) }
        try await repository.saveOperation({ failed }, { [] }).asyncExecute()
    }
}

private extension ChatPendingMessagesService {
    func pendingRepository(for contactAccountId: AccountId) -> AnyDataProviderRepository<Chat.ChatMessageStatusUpdate> {
        repositoryFactory.createRepository(forFilter: .newLocalDeviceMessages(to: .person(contactAccountId)))
    }
}
