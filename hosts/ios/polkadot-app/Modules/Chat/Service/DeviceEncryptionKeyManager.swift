import Foundation
import CryptoKit
import Keystore_iOS
import KeyDerivation
import TrUAPIHost

protocol DeviceEncryptionKeyManaging {
    func getOrCreatePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey
    func getPublicKey() throws -> Data
}

enum DeviceEncryptionKeyError: Error {
    case runtimeProviderMissing
}

/// This device's X25519 encryption key, kept by the TrUAPI core so chat and device sync encrypt
/// with the key the core names in `SessionUiInfo.deviceEncPublicKey`.
final class DeviceEncryptionKeyManager: DeviceEncryptionKeyManaging {
    static let shared = DeviceEncryptionKeyManager()

    private let keychain: KeystoreProtocol
    private let encryptionStorage: DeviceEncryptionStoring
    private let runtimeProvider: () -> TrUAPIHostRuntimeProviding?
    private let logger: LoggerProtocol

    private init(
        keychain: KeystoreProtocol = Keychain(),
        encryptionStorage: DeviceEncryptionStoring = DeviceEncryptionStorage(),
        runtimeProvider: @escaping () -> TrUAPIHostRuntimeProviding? = { RootDependencyLocator.getDependency() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.keychain = keychain
        self.encryptionStorage = encryptionStorage
        self.runtimeProvider = runtimeProvider
        self.logger = logger
    }

    /// The key this app generated before the core kept one, for `HostRuntimeConfig.deviceEncryptionKey`.
    /// `nil` once the core holds it.
    func keyToHandOver() throws -> Data? {
        do {
            return try keychain.fetchKey(for: appKeyTag)
        } catch KeystoreError.noKeyFound {
            return nil
        }
    }

    /// Forget the key from ``keyToHandOver()`` once a runtime has adopted it.
    func forgetHandedOverKey() {
        do {
            try keychain.deleteKey(for: appKeyTag)
        } catch {
            // The next launch hands the same key over again.
            logger.warning("Failed to delete the handed-over device encryption key: \(error)")
        }
    }

    func getOrCreatePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey {
        guard let runtimeProvider = runtimeProvider() else {
            throw DeviceEncryptionKeyError.runtimeProviderMissing
        }
        let key = try runtimeProvider.sharedRuntime().deviceEncryptionKey()
        return try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: key)
    }

    func getPublicKey() throws -> Data {
        try getOrCreatePrivateKey().publicKey.rawRepresentation
    }
}

private extension DeviceEncryptionKeyManager {
    var appKeyTag: String {
        KeystoreTag.deviceEncryptionKeyTag(for: encryptionStorage.deviceEncryptId)
    }
}
