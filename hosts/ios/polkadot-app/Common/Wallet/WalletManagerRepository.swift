import Foundation
import KeyDerivation
import Products

/// Resolves built-in account wallets. TLD-dependent accessors read the cached DotNs TLD and throw
/// if it is missing (can't happen after onboarding, where the TLD is resolved and cached);
/// TLD-independent accessors derive from fixed pallet-context paths and never fail.
protocol WalletManagerRepositoryProtocol {
    func main() throws -> WalletManaging
    func depositWallet() throws -> WalletManaging
    func resourcesAlias() -> WalletManaging
    func internalPayout() -> WalletManaging
    func bulletInForChat() -> WalletManaging
}

struct WalletManagerRepository: WalletManagerRepositoryProtocol {
    private let tldProvider: DotNsTldProviding

    init(tldProvider: DotNsTldProviding) {
        self.tldProvider = tldProvider
    }

    func main() throws -> WalletManaging {
        try SelectedWallet.main(for: tldProvider.currentTldOrError())
    }

    func depositWallet() throws -> WalletManaging {
        try SelectedWallet.depositWallet(for: tldProvider.currentTldOrError())
    }

    func resourcesAlias() -> WalletManaging {
        SelectedWallet.resourcesAlias()
    }

    func internalPayout() -> WalletManaging {
        SelectedWallet.internalPayout()
    }

    func bulletInForChat() -> WalletManaging {
        SelectedWallet.bulletInForChat()
    }
}

extension WalletManagerRepositoryProtocol where Self == WalletManagerRepository {
    static var shared: WalletManagerRepositoryProtocol {
        WalletManagerRepository(tldProvider: DotNsTldProviderFacade.shared)
    }
}
