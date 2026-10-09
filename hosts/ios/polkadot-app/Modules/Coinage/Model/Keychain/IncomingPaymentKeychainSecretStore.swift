import Coinage
import Foundation
import Keystore_iOS
import SubstrateSdk

/// Keychain-backed ``IncomingPaymentSecretStoring`` — the encrypted store for a top-up's source
/// material, the iOS counterpart of Android's encrypted preferences. Every read, write and removal
/// is namespaced by the persisted root/chain/instance owner, then operation group.
/// Legacy unscoped entries are deliberately untouched; their ownership is not provable.
final class IncomingPaymentKeychainSecretStore: IncomingPaymentSecretStoring, @unchecked Sendable {
    private let keyPrefix: String

    private let keychain: KeystoreProtocol
    private let logger: LoggerProtocol

    init(keychain: KeystoreProtocol, logger: LoggerProtocol, ownerId: Data) {
        self.keychain = keychain
        self.logger = logger
        keyPrefix = "topUpSource.v2." + ownerId.toHex() + "."
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
            logger.error("Top-up source could not be decoded")
            throw IncomingPaymentSecretStoreError.corrupted
        }
    }

    func remove(groupId: CoinageTxGroupId) {
        do {
            try keychain.deleteKeyIfExists(for: identifier(for: groupId))
        } catch {
            logger.error("Top-up source could not be removed")
        }
    }

    private func identifier(for groupId: CoinageTxGroupId) -> String {
        keyPrefix + groupId
    }
}
