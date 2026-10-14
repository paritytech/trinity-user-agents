import Foundation
import Operation_iOS
import SubstrateSdk

protocol BlockUserServicing {
    func blockUser(accountId: AccountId) async throws
    func unblockUser(accountId: AccountId) async throws
}

final class BlockUserService {
    private let blockStatusRepository: AnyDataProviderRepository<Chat.ContactBlockStatus>
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) {
        blockStatusRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(BlockContactMapper())
            )
        )

        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )
    }
}

extension BlockUserService: BlockUserServicing {
    func blockUser(accountId: AccountId) async throws {
        let status = Chat.ContactBlockStatus(accountId: accountId, isBlocked: true)
        try await blockStatusRepository.saveOperation({ [status] }, { [] }).asyncExecute()

        // Re-save the chat to notify NSFetchedResultsController subscribers
        // that filter by contact properties (e.g. contacts list)
        try await resaveChat(for: accountId)
    }

    func unblockUser(accountId: AccountId) async throws {
        let status = Chat.ContactBlockStatus(accountId: accountId, isBlocked: false)
        try await blockStatusRepository.saveOperation({ [status] }, { [] }).asyncExecute()

        // Re-save the chat to notify NSFetchedResultsController subscribers
        // that filter by contact properties (e.g. contacts list)
        try await resaveChat(for: accountId)
    }
}

private extension BlockUserService {
    func resaveChat(for accountId: AccountId) async throws {
        let chatId = Chat.Id.person(accountId)

        guard let chat = try await chatRepository
            .fetchOperation(
                by: { chatId.rawRepresentation },
                options: RepositoryFetchOptions()
            )
            .asyncExecute()
        else {
            return
        }

        try await chatRepository.saveOperation({ [chat] }, { [] }).asyncExecute()
    }
}
