import Foundation
import Keystore_iOS
import KeyDerivation
import TrUAPIHost
import UIKit

final class TrUAPISecretStorage: HostSecretStorageBackend, @unchecked Sendable {
    private let keychain: KeystoreProtocol

    init(keychain: KeystoreProtocol = Keychain()) {
        self.keychain = keychain
    }

    func read(key: SecretCoreStorageKey) async throws -> Data? {
        if case .deviceEncryptionKey = key {
            return try DeviceEncryptionKeyManager.shared.getOrCreatePrivateKey().rawRepresentation
        }
        do {
            return try keychain.fetchKey(for: identifier(key))
        } catch KeystoreError.noKeyFound {
            return nil
        }
    }

    func write(key: SecretCoreStorageKey, value: Data) async throws {
        if case .deviceEncryptionKey = key { throw HostRejection.Rejected(reason: "Shared device identity is owned by its protected provider") }
        try keychain.saveKey(value, with: identifier(key))
    }

    func clear(key: SecretCoreStorageKey) async throws {
        if case .deviceEncryptionKey = key { throw HostRejection.Rejected(reason: "Shared device identity cannot be removed by host cleanup") }
        try keychain.deleteKeyIfExists(for: identifier(key))
    }

    private func identifier(_ key: SecretCoreStorageKey) -> String {
        return secretCoreStorageKeyIdentifier(key: key)
    }
}

final class TrUAPIWalletSecrets: WalletSecretProvider, @unchecked Sendable {
    private let entropyManager: RootEntropyManaging
    private let installationStore: InstallationKeyIdStoring

    init(entropyManager: RootEntropyManaging, installationStore: InstallationKeyIdStoring = InstallationKeyIdStore()) {
        self.entropyManager = entropyManager
        self.installationStore = installationStore
    }

    func readWalletRootEntropy(walletId: String) async throws -> Data {
        guard await MainActor.run(body: { UIApplication.shared.isProtectedDataAvailable }) else {
            throw HostRejection.Rejected(reason: "Unlock the device before activating the wallet")
        }
        guard installationStore.getInstallationKeyId() == walletId else {
            throw RootEntropyManagerError.noEntropyFound
        }
        let entropy = try entropyManager.fetchRootEntropy()
        guard installationStore.getInstallationKeyId() == walletId,
              await MainActor.run(body: { UIApplication.shared.isProtectedDataAvailable }) else {
            throw HostRejection.Rejected(reason: "Wallet activation was invalidated")
        }
        return entropy
    }
}
