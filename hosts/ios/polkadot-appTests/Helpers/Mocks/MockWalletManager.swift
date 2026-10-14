import Foundation
@testable import polkadot_app
import KeyDerivation
import Keystore_iOS

enum MockWalletManager {
    static func mockedWallet(for derivationPath: String = "//wallet") throws -> WalletManaging {
        let keychain = InMemoryKeychain()
        let store = MockInstallationKeyIdStore()
        let manager = RootEntropyManager(keychain: keychain, installationKeyIdStore: store)
        let entropy = try Data.randomOrError(of: 32)
        try manager.createRootEntropy(entropy)

        return DynamicDerivedWallet(derivationPath: derivationPath, entropyManager: manager)
    }
}
