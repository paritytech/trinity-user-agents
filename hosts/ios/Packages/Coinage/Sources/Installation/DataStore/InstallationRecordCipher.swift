import CryptoKit
import Foundation
import SubstrateSdk

/// The on-chain form of an installation id: ChaCha20-Poly1305 `nonce(12) || ciphertext(32) || tag(16)`.
///
/// Sealing is deterministic — the nonce is a keyed hash of the id — so every attempt at registering one
/// installation submits the same bytes and the contract stores it once. The hash is keyed by a subkey
/// rather than the cipher key itself, keeping the two primitives on separate keys. Opening reads the
/// nonce from the record, so records sealed with any nonce, e.g. random ones from another platform,
/// open all the same.
public enum InstallationRecordCipher {
    static let nonceSize = 12
    static let tagSize = 16
    static let recordSize = nonceSize + CoinageInstallationId.sizeBytes + tagSize

    private static let nonceKeyContext = Data("nonce-key".utf8)
    private static let nonceContext = Data("nonce".utf8)

    public static func seal(_ installation: CoinageInstallationId, key: Data) throws -> Data {
        let nonce = try ChaChaPoly.Nonce(data: nonceOf(installation, key: key))
        let box = try ChaChaPoly.seal(installation.value, using: SymmetricKey(data: key), nonce: nonce)
        return box.combined
    }

    /// `nil` for anything this key did not seal, which is what a record written under another seed
    /// looks like.
    public static func open(_ record: Data, key: Data) -> CoinageInstallationId? {
        guard record.count == recordSize,
              let box = try? ChaChaPoly.SealedBox(combined: record),
              let plain = try? ChaChaPoly.open(box, using: SymmetricKey(data: key))
        else { return nil }
        return try? CoinageInstallationId(value: plain)
    }

    private static func nonceOf(_ installation: CoinageInstallationId, key: Data) throws -> Data {
        let nonceKey = try nonceKeyContext.blake2b32WithKey(key)
        return try (nonceContext + installation.value).blake2b32WithKey(nonceKey).prefix(nonceSize)
    }
}
