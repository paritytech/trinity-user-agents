import Foundation
import SubstrateSdk

enum ContactSearchPayload {
    case local(Chat.Contact)
    case remote(Chat.RemoteContact)

    var accountId: AccountId {
        switch self {
        case let .local(contact): contact.accountId
        case let .remote(contact): contact.accountId
        }
    }

    var username: String {
        switch self {
        case let .local(contact): contact.username
        case let .remote(contact): contact.username
        }
    }
}
