@testable import polkadot_app
import Foundation
import Operation_iOS
import SubstrateSdk

final class MockLocalContactSearch: LocalContactSearching {
    var contacts: [Chat.Contact] = []
    var contactsError: Error?
    var blockedContactsError: Error?

    // Recorded inputs
    var receivedUsernamePrefix: String?
    var receivedAccountId: AccountId?
    var didRequestAllContacts: Bool = false
    var didRequestBlockedContacts: Bool = false

    func searchContacts(usernamePrefix: String) -> AnyDataProviderRepository<Chat.Contact> {
        receivedUsernamePrefix = usernamePrefix
        return makeSeededRepository()
    }

    func contact(accountId: AccountId) -> AnyDataProviderRepository<Chat.Contact> {
        receivedAccountId = accountId
        return makeSeededRepository()
    }

    func allContacts() -> AnyDataProviderRepository<Chat.Contact> {
        didRequestAllContacts = true
        return makeSeededRepository()
    }

    func blockedContacts() -> AnyDataProviderRepository<Chat.Contact> {
        didRequestBlockedContacts = true

        if let blockedContactsError {
            return makeFailingRepository(error: blockedContactsError)
        }

        return makeRepository(with: contacts.filter(\.isBlocked))
    }

    private func makeRepository(with contacts: [Chat.Contact]) -> AnyDataProviderRepository<Chat.Contact> {
        let repository = InMemoryDataProviderRepository<Chat.Contact>()
        // `start()` runs the operation inline; an OperationQueue wait here would block
        // a cooperative-pool thread, since callers seed from an async context.
        repository.replaceOperation { contacts }.start()
        return AnyDataProviderRepository(repository)
    }

    private func makeSeededRepository() -> AnyDataProviderRepository<Chat.Contact> {
        if let contactsError {
            return makeFailingRepository(error: contactsError)
        }

        return makeRepository(with: contacts)
    }

    private func makeFailingRepository(error: Error) -> AnyDataProviderRepository<Chat.Contact> {
        AnyDataProviderRepository(FailingContactRepository(error: error))
    }
}

/// Every operation throws, so a test can drive the failure branch of a repository fetch.
private final class FailingContactRepository: DataProviderRepositoryProtocol {
    typealias Model = Chat.Contact

    private let error: Error

    init(error: Error) {
        self.error = error
    }

    func fetchOperation(
        by _: @escaping () throws -> String,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<Chat.Contact?> {
        failingOperation()
    }

    func fetchAllOperation(with _: RepositoryFetchOptions) -> BaseOperation<[Chat.Contact]> {
        failingOperation()
    }

    func fetchOperation(
        by _: RepositorySliceRequest,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<[Chat.Contact]> {
        failingOperation()
    }

    func saveOperation(
        _: @escaping () throws -> [Chat.Contact],
        _: @escaping () throws -> [String]
    ) -> BaseOperation<Void> {
        failingOperation()
    }

    func replaceOperation(_: @escaping () throws -> [Chat.Contact]) -> BaseOperation<Void> {
        failingOperation()
    }

    func fetchCountOperation() -> BaseOperation<Int> {
        failingOperation()
    }

    func deleteAllOperation() -> BaseOperation<Void> {
        failingOperation()
    }

    private func failingOperation<T>() -> BaseOperation<T> {
        ClosureOperation { [error] in throw error }
    }
}
