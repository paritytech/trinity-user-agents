import Foundation
import Individuality
import KeyDerivation

/// Built-in account wallets. TLD-dependent accounts (`main`/`depositWallet`)
/// derive from a product domain suffixed with the DotNs TLD and must be resolved through
/// `WalletManagerRepositoryProtocol`; the TLD-independent accounts use fixed pallet-context paths.
enum SelectedWallet {
    static func main(for tld: String) -> DynamicDerivedWallet {
        DynamicDerivedWallet(derivationPath: WalletDerivationPath.main(for: tld))
    }

    static func depositWallet(for tld: String) -> DynamicDerivedWallet {
        DynamicDerivedWallet(derivationPath: WalletDerivationPath.deposit(for: tld))
    }

    static func resourcesAlias() -> DynamicDerivedWallet {
        DynamicDerivedWallet(derivationPath: "//\(PalletContext.resources)")
    }

    static func internalPayout() -> DynamicDerivedWallet {
        DynamicDerivedWallet(derivationPath: "//\(PalletContext.privacyVoucher)")
    }

    static func bulletInForChat() -> DynamicDerivedWallet {
        DynamicDerivedWallet(derivationPath: WalletDerivationPath.bulletInForChat)
    }
}
