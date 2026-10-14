import Foundation
import Keystore_iOS

public protocol RootEntropyManaging {
    func fetchRootEntropy() throws -> Data
    func createRootEntropy(_ entropy: Data) throws
    func hasRootEntropy() throws -> Bool
}

/// The app installation key id: a UUID minted whenever root entropy is created. It scopes every
/// Keychain item that belongs to one wallet (root entropy, coinage page and counters), so a new
/// wallet never reads another wallet's items. The app persists it in UserDefaults.
public protocol InstallationKeyIdStoring: Sendable {
    func saveInstallationKeyId(_ installationKeyId: String)
    func getInstallationKeyId() -> String?
}

public enum RootEntropyManagerError: Error {
    case noEntropyFound
}

public final class RootEntropyManager {
    let keychain: KeystoreProtocol
    let installationKeyIdStore: InstallationKeyIdStoring

    public init(
        keychain: KeystoreProtocol,
        installationKeyIdStore: InstallationKeyIdStoring
    ) {
        self.keychain = keychain
        self.installationKeyIdStore = installationKeyIdStore
    }
}

extension RootEntropyManager: RootEntropyManaging {
    public func hasRootEntropy() throws -> Bool {
        guard let installationKeyId = installationKeyIdStore.getInstallationKeyId() else {
            return false
        }

        return try keychain.checkKey(for: KeystoreTag.rootEntropyTag(for: installationKeyId))
    }

    public func fetchRootEntropy() throws -> Data {
        guard let installationKeyId = installationKeyIdStore.getInstallationKeyId() else {
            throw RootEntropyManagerError.noEntropyFound
        }

        return try keychain.fetchKey(for: KeystoreTag.rootEntropyTag(for: installationKeyId))
    }

    public func createRootEntropy(_ entropy: Data) throws {
        let newInstallationKeyId = UUID().uuidString
        installationKeyIdStore.saveInstallationKeyId(newInstallationKeyId)
        try keychain.saveKey(entropy, with: KeystoreTag.rootEntropyTag(for: newInstallationKeyId))
    }
}
