import Foundation
import KeyDerivation

extension RootGate {
    struct Wallet: DecisionGate {
        private let entropyManager: RootEntropyManaging
        private let backupHelper: MnemonicBackupHelperProtocol

        init(entropyManager: RootEntropyManaging, backupHelper: MnemonicBackupHelperProtocol) {
            self.entropyManager = entropyManager
            self.backupHelper = backupHelper
        }

        func evaluate() throws -> RootDestination? {
            guard try !entropyManager.hasRootEntropy() else {
                return nil
            }

            let hasBackup = (try? backupHelper.checkForBackup()) ?? false
            return hasBackup ? .restoreFromCloud : .onboarding
        }
    }
}
