import KeyDerivation

/// A protocol for models that can be derived using an index.
public protocol CoinageDerivable {
    var derivationIndex: CoinageKeyIndex { get }
}

public protocol CoinKeypairFactoryProtocol {
    /// Derives the public key for a derivation index.
    func derivePublicKey(index: CoinageKeyIndex) throws -> PublicKey

    /// Derives the private key for a derivation index.
    func derivePrivateKey(index: CoinageKeyIndex) throws -> PrivateKey
}

public extension CoinKeypairFactoryProtocol {
    /// Derives the private key for a model from its derivation index.
    func derivePrivateKey(for model: Coin) throws -> PrivateKey {
        try derivePrivateKey(index: model.derivationIndex)
    }
}

public protocol VoucherKeypairFactoryProtocol {
    /// Derives the public key for a derivation index.
    func derivePublicKey(index: CoinageKeyIndex) throws -> PublicKey
}
