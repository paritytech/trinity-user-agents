import Foundation
@testable import polkadot_app
import KeyDerivation
import Keystore_iOS
import Individuality
import SubstrateSdk

/// Derives real wallets over an in-memory entropy manager for a fixed TLD, so consumer tests get
/// deterministic `WalletManaging` instances without touching the keychain or the DotNs chain.
final class MockWalletManagerRepository: WalletManagerRepositoryProtocol {
    let tld: String
    let entropyManager: RootEntropyManaging

    init(tld: String = "dot", entropyManager: RootEntropyManaging) {
        self.tld = tld
        self.entropyManager = entropyManager
    }

    convenience init(tld: String = "dot") throws {
        let manager = RootEntropyManager(
            keychain: InMemoryKeychain(),
            installationKeyIdStore: MockInstallationKeyIdStore()
        )
        try manager.createRootEntropy(Data.randomOrError(of: 32))
        self.init(tld: tld, entropyManager: manager)
    }

    func main() throws -> WalletManaging { wallet(WalletDerivationPath.main(for: tld)) }
    func depositWallet() throws -> WalletManaging { wallet(WalletDerivationPath.deposit(for: tld)) }
    func resourcesAlias() -> WalletManaging { wallet("//\(PalletContext.resources)") }
    func internalPayout() -> WalletManaging { wallet("//\(PalletContext.privacyVoucher)") }
    func bulletInForChat() -> WalletManaging { wallet(WalletDerivationPath.bulletInForChat) }

    private func wallet(_ derivationPath: String) -> WalletManaging {
        DynamicDerivedWallet(derivationPath: derivationPath, entropyManager: entropyManager)
    }
}
