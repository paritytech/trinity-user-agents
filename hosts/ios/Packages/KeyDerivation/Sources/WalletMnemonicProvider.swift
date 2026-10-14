import Foundation
import NovaCrypto

public protocol WalletMnemonicProviding {
    func fetchMnemonic() throws -> String
}

public final class WalletKeystoreMnemonicProvider {
    let entropyManager: RootEntropyManaging

    private lazy var mnemonicGenerator = IRMnemonicCreator()

    public init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }
}

extension WalletKeystoreMnemonicProvider: WalletMnemonicProviding {
    public func fetchMnemonic() throws -> String {
        let entropy = try entropyManager.fetchRootEntropy()
        let mnemonic = try mnemonicGenerator.mnemonic(fromEntropy: entropy)
        return mnemonic.toString()
    }
}

public final class WalletMnemonicProvider {
    let mnemonic: String

    public init(mnemonic: String) {
        self.mnemonic = mnemonic
    }
}

extension WalletMnemonicProvider: WalletMnemonicProviding {
    public func fetchMnemonic() throws -> String {
        mnemonic
    }
}
