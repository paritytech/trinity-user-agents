import Coinage
import Foundation
import Keystore_iOS
import SubstrateSdk

/// Keychain-backed ``IncomingPaymentSecretStoring`` — the encrypted store for a top-up's source
/// material, the iOS counterpart of Android's encrypted preferences. One entry per operation, keyed
/// by `groupId`, removed the moment a verdict is reached.
final class IncomingPaymentKeychainSecretStore: IncomingPaymentSecretStoring, @unchecked Sendable {
    private static let keyPrefix = "topUpSource."

    private let keychain: KeystoreProtocol
    private let logger: LoggerProtocol

    init(keychain: KeystoreProtocol, logger: LoggerProtocol) {
        self.keychain = keychain
        self.logger = logger
    }

    func save(groupId: CoinageTxGroupId, descriptor: IncomingPaymentSourceDescriptor) throws {
        let data = try JSONEncoder().encode(descriptor)
        try keychain.saveKey(data, with: identifier(for: groupId))
    }

    /// Only a missing item reads as `nil`; a Keychain that cannot be read (locked before first unlock
    /// on a VoIP-push launch, say) throws its own error, so the caller never mistakes it for a lost
    /// secret. An entry that no longer decodes is `corrupted`: no launch will read it, so the caller
    /// treats it like a lost one rather than pinning the payment active forever.
    func fetch(groupId: CoinageTxGroupId) throws -> IncomingPaymentSourceDescriptor? {
        let data: Data
        do {
            data = try keychain.fetchKey(for: identifier(for: groupId))
        } catch KeystoreError.noKeyFound {
            return nil
        }

        do {
            return try JSONDecoder().decode(IncomingPaymentSourceDescriptor.self, from: data)
        } catch {
            logger.error("Top-up secret for \(groupId) does not decode: \(error)")
            throw IncomingPaymentSecretStoreError.corrupted
        }
    }

    func remove(groupId: CoinageTxGroupId) {
        do {
            try keychain.deleteKeyIfExists(for: identifier(for: groupId))
        } catch {
            logger.error("Top-up secret for \(groupId) could not be removed: \(error)")
        }
    }

    private func identifier(for groupId: CoinageTxGroupId) -> String {
        Self.keyPrefix + groupId
    }
}
