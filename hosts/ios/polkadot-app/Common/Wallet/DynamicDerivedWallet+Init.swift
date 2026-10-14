import Foundation
import KeyDerivation

extension DynamicDerivedWallet {
    init(derivationPath: String?) {
        self.init(derivationPath: derivationPath, entropyManager: RootEntropyManager.shared)
    }
}
