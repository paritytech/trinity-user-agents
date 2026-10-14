import Foundation
import KeyDerivation
import SubstrateSdk

enum IdentityAccountResolverError: Error {
    case accountMismatch
}

protocol IdentityAccountResolving {
    @discardableResult
    func resolveWallet(for accountId: AccountId) throws -> WalletManaging
}

struct IdentityAccountResolver: IdentityAccountResolving {
    private let walletRepo: WalletManagerRepositoryProtocol

    init(walletRepo: WalletManagerRepositoryProtocol = .shared) {
        self.walletRepo = walletRepo
    }

    @discardableResult
    func resolveWallet(for accountId: AccountId) throws -> WalletManaging {
        let identityWallet = try walletRepo.main()
        let identityAccountId = try identityWallet.getRawPublicKey()

        guard identityAccountId == accountId else {
            throw IdentityAccountResolverError.accountMismatch
        }

        return identityWallet
    }
}
