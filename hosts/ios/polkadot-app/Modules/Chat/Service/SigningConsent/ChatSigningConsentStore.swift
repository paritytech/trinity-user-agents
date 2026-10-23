import Foundation
import FoundationExt
import Operation_iOS
import SubstrateSdk

protocol ChatSigningConsentStoring: Sendable {
    func hasConsent(for contactAccountId: AccountId) async throws -> Bool
    func grant(for contactAccountId: AccountId) async throws
    func revoke(for contactAccountId: AccountId) async throws
}

final class ChatSigningConsentStore: @unchecked Sendable {
    private let repository: AnyDataProviderRepository<Chat.IdentitySigningConsent>
    private let dateProvider: DateProviding

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        dateProvider: DateProviding = NowDateProvider()
    ) {
        repository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatIdentitySigningConsentMapper())
            )
        )
        self.dateProvider = dateProvider
    }
}

extension ChatSigningConsentStore: ChatSigningConsentStoring {
    func hasConsent(for contactAccountId: AccountId) async throws -> Bool {
        try await repository
            .fetchOperation(by: { contactAccountId.toHex() }, options: RepositoryFetchOptions())
            .asyncExecute() != nil
    }

    func grant(for contactAccountId: AccountId) async throws {
        let consent = await Chat.IdentitySigningConsent(
            contactAccountId: contactAccountId,
            grantedAt: dateProvider.read()
        )

        try await repository.saveOperation({ [consent] }, { [] }).asyncExecute()
    }

    func revoke(for contactAccountId: AccountId) async throws {
        try await repository.saveOperation({ [] }, { [contactAccountId.toHex()] }).asyncExecute()
    }
}
