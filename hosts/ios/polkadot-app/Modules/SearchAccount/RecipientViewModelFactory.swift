import Foundation
import SubstrateSdkExt

struct RecipientViewModel: Hashable {
    let accountType: SearchAccountViewModel.AccountType
    let recentContactID: String
}

protocol RecipientViewModelFactoryProtocol {
    func createRecentContacts(from contacts: [RecentContactModelWithUsername]) -> [RecipientViewModel]
}

final class RecipientViewModelFactory: RecipientViewModelFactoryProtocol {
    func createRecentContacts(from contacts: [RecentContactModelWithUsername]) -> [RecipientViewModel] {
        contacts.compactMap { item in
            guard
                let chainFormat = item.chainAsset?.chain.chainFormat,
                let accountAddress = try? item.recentContact.accountID.toAddress(using: chainFormat)
            else {
                return nil
            }

            let accountType: SearchAccountViewModel.AccountType =
                if let username = item.username,
                !username.value.isEmpty {
                    .username(username.value, accountAddress)
                } else {
                    .accountAddress(accountAddress)
                }

            return RecipientViewModel(accountType: accountType, recentContactID: item.recentContact.identifier)
        }
    }
}
