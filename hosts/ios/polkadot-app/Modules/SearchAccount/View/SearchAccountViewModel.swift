import UIKit
import Foundation_iOS
import SubstrateSdk

struct SearchAccountViewModel {
    let inputViewModel: InputModel
    let content: Content

    init(
        inputViewModel: InputModel = InputModel(
            inputViewModel: InputViewModel.createAccountInputViewModel(for: "")
        ),
        content: Content = .empty
    ) {
        self.inputViewModel = inputViewModel
        self.content = content
    }
}

extension SearchAccountViewModel {
    enum Section: Hashable {
        case selfTransfer
        case contacts
        case recentContacts
        case globalSearch
    }

    enum AccountType: Hashable {
        case username(String, AccountAddress)
        case accountAddress(AccountAddress)
    }

    struct InputModel {
        let inputViewModel: InputViewModelProtocol
        var selectedAccount: AccountType?
    }

    struct Status {
        let message: String?
        let showsLoader: Bool
    }

    struct Content {
        let recent: [RecipientViewModel]
        let contacts: [AccountType]
        let global: [AccountType]
        var showsSelfTransfer: Bool

        static let empty = Content(recent: [], contacts: [], global: [], showsSelfTransfer: false)
    }
}

extension SearchAccountViewModel.AccountType {
    var title: String {
        switch self {
        case let .username(username, _): username
        case let .accountAddress(accountAddress): accountAddress
        }
    }

    var accountAddress: AccountAddress {
        switch self {
        case let .username(_, accountAddress),
             let .accountAddress(accountAddress):
            accountAddress
        }
    }
}

extension SearchAccountViewModel.Section {
    var title: String? {
        switch self {
        case .selfTransfer: nil
        case .contacts: String(localized: .transactionSearchMyContacts)
        case .recentContacts: String(localized: .transactionSearchRecentContacts)
        case .globalSearch: String(localized: .transactionSearchAllUsers)
        }
    }
}
