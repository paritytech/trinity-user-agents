import Foundation
import SubstrateSdk
import Operation_iOS

/// The contacts the user keeps, for the contact searches of the app. The chat product keeps them (HX2).
protocol LocalContactSearching {
    func searchContacts(usernamePrefix: String) -> AnyDataProviderRepository<Chat.Contact>
    func contact(accountId: AccountId) -> AnyDataProviderRepository<Chat.Contact>
    func allContacts() -> AnyDataProviderRepository<Chat.Contact>
    func blockedContacts() -> AnyDataProviderRepository<Chat.Contact>
}
