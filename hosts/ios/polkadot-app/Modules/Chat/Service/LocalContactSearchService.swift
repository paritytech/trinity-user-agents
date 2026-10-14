import Foundation
import SubstrateSdk
import Operation_iOS

protocol LocalContactSearching {
    func searchContacts(usernamePrefix: String) -> AnyDataProviderRepository<Chat.Contact>
    func contact(accountId: AccountId) -> AnyDataProviderRepository<Chat.Contact>
    func allContacts() -> AnyDataProviderRepository<Chat.Contact>
    func blockedContacts() -> AnyDataProviderRepository<Chat.Contact>
}

final class LocalContactSearchService: LocalContactSearching {
    private let repositoryFactory: ChatContactRepositoryMaking

    init(repositoryFactory: ChatContactRepositoryMaking) {
        self.repositoryFactory = repositoryFactory
    }

    func searchContacts(usernamePrefix: String) -> AnyDataProviderRepository<Chat.Contact> {
        let predicate = NSPredicate.contact(beginsWith: usernamePrefix)
        return repositoryFactory.createRepository(forFilter: predicate)
    }

    func contact(accountId: AccountId) -> AnyDataProviderRepository<Chat.Contact> {
        let predicate = NSPredicate.contact(accountId: accountId)
        return repositoryFactory.createRepository(forFilter: predicate)
    }

    func allContacts() -> AnyDataProviderRepository<Chat.Contact> {
        let predicate = NSPredicate.isContact()
        return repositoryFactory.createRepository(forFilter: predicate)
    }

    func blockedContacts() -> AnyDataProviderRepository<Chat.Contact> {
        repositoryFactory.createRepository(forFilter: .blockedContacts())
    }
}
