import Foundation
import Keystore_iOS
import SubstrateSdk

/// Keychain-backed values the Rust core keeps encrypted at rest, such as the
/// secrets of NFT receive keys. `Keychain()` items are `ThisDeviceOnly`, so they
/// never reach another device. One item per core key, hex-encoded after a prefix.
final class TrUAPIEncryptedStorage: @unchecked Sendable {
    private static let keyPrefix = "truapi.encrypted."

    private let keychain: KeystoreProtocol

    init(keychain: KeystoreProtocol) {
        self.keychain = keychain
    }

    /// Only a missing item reads as `nil`; a Keychain that cannot be read
    /// (locked before first unlock, say) throws, so the core never mistakes it
    /// for a lost secret.
    func read(key: Data) throws -> Data? {
        let identifier = identifier(for: key)
        do {
            return try keychain.fetchKey(for: identifier)
        } catch KeystoreError.noKeyFound {
            return nil
        }
    }

    /// Stores `value`, replacing an earlier value under the same key.
    func write(key: Data, value: Data) throws {
        let identifier = identifier(for: key)
        try keychain.saveKey(value, with: identifier)
    }

    private func identifier(for key: Data) -> String {
        Self.keyPrefix + key.toHex()
    }
}
