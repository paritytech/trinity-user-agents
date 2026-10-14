import Foundation
import SubstrateSdk
import ChainRegistry
import SubstrateSdkExt

struct RecipientModel {
    let accountId: AccountId
    let username: String?
}

extension RecipientModel {
    init(accountType: SearchAccountViewModel.AccountType) throws {
        switch accountType {
        case let .username(username, address):
            accountId = try address.toAccountId()
            self.username = username
        case let .accountAddress(address):
            accountId = try address.toAccountId()
            username = nil
        }
    }

    func address(in chain: ChainModel) -> AccountAddress? {
        try? accountId.toAddress(using: chain.chainFormat)
    }
}
