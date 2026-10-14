@preconcurrency import KeyDerivation
import SubstrateSdk
import NovaCrypto

public protocol CoinKeyDeriving: CoinKeypairFactoryProtocol, Sendable {}

public final class CoinKeypairFactory {
    let entropyManager: RootEntropyManaging

    public init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }
}

extension CoinKeypairFactory: CoinKeyDeriving {
    public func derivePublicKey(index: CoinageKeyIndex) throws -> PublicKey {
        try WalletMnemonicKeypairFactory(
            derivationPath: coinPath(for: index),
            entropyManager: entropyManager
        )
        .derivePublicKey()
        .rawData()
    }

    public func derivePrivateKey(index: CoinageKeyIndex) throws -> PrivateKey {
        try WalletMnemonicKeypairFactory(
            derivationPath: coinPath(for: index),
            entropyManager: entropyManager
        )
        .deriveKeypair()
        .privateKey()
        .rawData()
    }
}

public extension CoinKeypairFactory {
    func coinPath(for derivationIndex: CoinageKeyIndex) -> String {
        let purse = CoinageConstants.Derivation.mainPurse
        let page = derivationIndex.installation.pageSegment

        return "//coinage//\(purse)//\(page)/\(derivationIndex.item)"
    }
}
