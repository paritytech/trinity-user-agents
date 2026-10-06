import Foundation
import Keystore_iOS
import Products
import TrUAPIHost

final class TrUAPISecretStorage: HostSecretStorageBackend, @unchecked Sendable {
    private let keychain: KeystoreProtocol
    private let storeIdProvider: ProductResourceStoreIdProviding
    private let deviceKeys: DeviceEncryptionKeyManaging
    private let lock: NSLock
    private var retired = false

    init(
        keychain: KeystoreProtocol,
        storeIdProvider: ProductResourceStoreIdProviding,
        deviceKeys: DeviceEncryptionKeyManaging,
        lock: NSLock
    ) {
        self.keychain = keychain
        self.storeIdProvider = storeIdProvider
        self.deviceKeys = deviceKeys
        self.lock = lock
    }

    func read(key: SecretCoreStorageKey) async throws -> Data? {
        try lock.withLock {
            try Task.checkCancellation()
            guard !retired else { throw HostRejection.Rejected(reason: "Protected storage is retired") }
            if case .deviceEncryptionKey = key {
                return try deviceKeys.getOrCreatePrivateKey().rawRepresentation
            }
            do {
                return try keychain.fetchKey(for: identifier(key))
            } catch KeystoreError.noKeyFound {
                return nil
            }
        }
    }

    func write(key: SecretCoreStorageKey, value: Data) async throws {
        try lock.withLock {
            try Task.checkCancellation()
            guard !retired else { throw HostRejection.Rejected(reason: "Protected storage is retired") }
            if case .deviceEncryptionKey = key {
                throw HostRejection.Rejected(reason: "Device encryption identity is managed by the device key provider")
            }
            try keychain.saveKey(value, with: identifier(key))
        }
    }

    func clear(key: SecretCoreStorageKey) async throws {
        try lock.withLock {
            try Task.checkCancellation()
            guard !retired else { throw HostRejection.Rejected(reason: "Protected storage is retired") }
            if case .deviceEncryptionKey = key {
                throw HostRejection.Rejected(reason: "Device encryption identity is managed by the device key provider")
            }
            try keychain.deleteKeyIfExists(for: identifier(key))
        }
    }

    func retire() {
        lock.withLock { retired = true }
    }

    private func identifier(_ key: SecretCoreStorageKey) -> String {
        "io.polkadotapp:\(storeIdProvider.getStoreId()):\(secretCoreStorageKeyIdentifier(key: key))"
    }
}
