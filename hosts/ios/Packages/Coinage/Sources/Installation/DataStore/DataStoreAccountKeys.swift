import Foundation
import NovaCrypto
import KeyDerivation
import Revive
import SubstrateSdk

/// The `//datastore` sr25519 account that owns this seed's list in the `AccountDataStore` contract,
/// with the key its records are sealed under.
public struct DataStoreAccount: Sendable {
    public let privateKey: PrivateKey
    public let publicKey: PublicKey
    /// The pallet-revive H160 the contract keys the list by: the last 20 bytes of `keccak256(accountId)`.
    public let evmAccountId: EvmAddress
    public let encryptionKey: Data

    public var accountId: AccountId { publicKey }

    public init(privateKey: PrivateKey, publicKey: PublicKey, evmAccountId: EvmAddress, encryptionKey: Data) {
        self.privateKey = privateKey
        self.publicKey = publicKey
        self.evmAccountId = evmAccountId
        self.encryptionKey = encryptionKey
    }
}

public protocol DataStoreAccountKeysProviding: Sendable {
    func account() async throws -> DataStoreAccount
}

/// Derives the data store account once from the root entropy and caches it.
public actor DataStoreAccountKeys: DataStoreAccountKeysProviding {
    public static let derivationPath = "//datastore"
    private static let encryptionContext = Data("encryption".utf8)

    private let entropyManager: RootEntropyManaging
    private var derived: DataStoreAccount?

    public init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }

    public func account() async throws -> DataStoreAccount {
        if let derived { return derived }

        let keypair = try WalletMnemonicKeypairFactory(
            derivationPath: Self.derivationPath,
            entropyManager: entropyManager
        ).deriveKeypair()
        let publicKey = keypair.publicKey().rawData()
        let secret = keypair.privateKey().rawData()

        // `secret` is the canonical scalar the iOS SDK signs with; the encryption key is keyed by the
        // form polkadot-js and Android hold, so both platforms open each other's records.
        let account = try DataStoreAccount(
            privateKey: secret,
            publicKey: publicKey,
            evmAccountId: publicKey.toH160(),
            encryptionKey: Self.deriveEncryptionKey(sr25519Secret: Self.ed25519Form(of: secret))
        )
        derived = account
        return account
    }

    /// Keyed by the full 64-byte sr25519 secret in schnorrkel's ed25519 form — the scalar times the
    /// cofactor, then its nonce — which is what polkadot-js and Android derive from the same `//datastore`
    /// keypair. ``ed25519Form(of:)`` converts the iOS SDK's canonical scalar.
    public static func deriveEncryptionKey(sr25519Secret: Data) throws -> Data {
        try encryptionContext.blake2b32WithKey(sr25519Secret)
    }

    /// The secret as polkadot-js and the Android SDK hold it (`sr25519_to_ed25519_bytes`).
    public static func ed25519Form(of sr25519Secret: Data) throws -> Data {
        guard let converted = try SNPrivateKey(rawData: sr25519Secret).toEd25519Data() else {
            throw DataStoreAccountKeysError.secretConversionFailed
        }
        return converted
    }
}

public enum DataStoreAccountKeysError: Error, Equatable {
    case secretConversionFailed
}
