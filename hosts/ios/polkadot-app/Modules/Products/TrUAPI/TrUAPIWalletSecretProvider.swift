import Foundation
import KeyDerivation
import TrUAPIHost

final class TrUAPIWalletSecretProvider: NativeWalletSecretProvider, @unchecked Sendable {
    private let entropyManager: RootEntropyManaging
    private let installationKeyIdStore: InstallationKeyIdStoring

    init(entropyManager: RootEntropyManaging, installationKeyIdStore: InstallationKeyIdStoring) {
        self.entropyManager = entropyManager
        self.installationKeyIdStore = installationKeyIdStore
    }

    func readWalletRootEntropy(walletId: String) async throws -> Data {
        do {
            try Task.checkCancellation()
            guard installationKeyIdStore.getInstallationKeyId() == walletId else {
                throw HostRejection.Rejected(reason: "Wallet selection changed")
            }
            let entropy = try entropyManager.fetchRootEntropy(installationKeyId: walletId)
            try Task.checkCancellation()
            guard installationKeyIdStore.getInstallationKeyId() == walletId else {
                throw HostRejection.Rejected(reason: "Wallet selection changed")
            }
            return entropy
        } catch let rejection as HostRejection {
            throw rejection
        } catch {
            throw HostRejection.Rejected(reason: String(describing: error))
        }
    }
}
