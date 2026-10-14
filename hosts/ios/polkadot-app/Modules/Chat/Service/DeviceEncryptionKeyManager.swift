import Foundation
import CryptoKit
import Keystore_iOS
import KeyDerivation

protocol DeviceEncryptionKeyManaging {
    func getOrCreatePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey
    func getPublicKey() throws -> Data
}

final class DeviceEncryptionKeyManager: DeviceEncryptionKeyManaging {
    static let shared = DeviceEncryptionKeyManager()

    private let keychain: KeystoreProtocol
    private let encryptionStorage: DeviceEncryptionStoring
    private let logger: LoggerProtocol
    private let lock = NSLock()

    private init(
        keychain: KeystoreProtocol = Keychain(),
        encryptionStorage: DeviceEncryptionStoring = DeviceEncryptionStorage(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.keychain = keychain
        self.encryptionStorage = encryptionStorage
        self.logger = logger
    }

    func getOrCreatePrivateKey() throws -> Curve25519.KeyAgreement.PrivateKey {
        lock.lock()
        defer { lock.unlock() }

        let tag = KeystoreTag.deviceEncryptionKeyTag(for: encryptionStorage.deviceEncryptId)

        do {
            let existingKeyData = try keychain.fetchKey(for: tag)
            return try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: existingKeyData)
        } catch KeystoreError.noKeyFound {
            logger.debug("Device encryption key not found, generating new key")
            let privateKey = Curve25519.KeyAgreement.PrivateKey()
            try keychain.saveKey(privateKey.rawRepresentation, with: tag)
            logger.debug("Device encryption key generated and saved")
            return privateKey
        } catch {
            logger.error("Failed to fetch device encryption key: \(error)")
            throw error
        }
    }

    func getPublicKey() throws -> Data {
        let privateKey = try getOrCreatePrivateKey()
        return privateKey.publicKey.rawRepresentation
    }
}
