import Foundation
import Operation_iOS
import SubstrateSdk

protocol ChatRemovalCleanupServicing {
    func cleanUpAfterRemoval(of accountId: AccountId) async throws
}

final class ChatRemovalCleanupService {
    private let allowanceCleanupService: AllowanceCleanupServicing
    private let signingConsentStore: ChatSigningConsentStoring
    private let removedChatRepository: AnyDataProviderRepository<Chat.RemovedChat>

    init(
        allowanceCleanupService: AllowanceCleanupServicing,
        signingConsentStore: ChatSigningConsentStoring,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) {
        self.allowanceCleanupService = allowanceCleanupService
        self.signingConsentStore = signingConsentStore

        removedChatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(RemovedChatMapper())
            )
        )
    }
}

extension ChatRemovalCleanupService: ChatRemovalCleanupServicing {
    func cleanUpAfterRemoval(of accountId: AccountId) async throws {
        try await saveRemovedChatTombstone(for: accountId)
        try await signingConsentStore.revoke(for: accountId)

        await allowanceCleanupService.cleanup()
    }
}

private extension ChatRemovalCleanupService {
    func saveRemovedChatTombstone(for accountId: AccountId) async throws {
        let tombstone = Chat.RemovedChat(
            accountId: accountId,
            removedAt: Date()
        )
        try await removedChatRepository.saveOperation({ [tombstone] }, { [] }).asyncExecute()
    }
}
