import Foundation
import KeyDerivation

/// Detects a device backup restored onto another device and drops the wallet state it carried.
///
/// The installation key id comes back with the App Group defaults, but the root entropy it indexes is a
/// this-device-only Keychain item and does not. The wallet gate already sends such a launch to onboarding
/// or iCloud recovery, and wallet creation writes a new key id; what would survive is the old wallet's
/// identity and progress in UserDefaults — the username gate would pass the new wallet under the old
/// name — so those values are erased before the gates read them. The CoreData directory is
/// backup-excluded and is not carried over; the Keychain is not restored at all.
/// A same-device restore brings the Keychain back as well and matches nothing here. A Keychain read
/// error propagates: nothing is erased on an unknown state.
final class RestoredBackupGuard: Migrating {
    private let keyIdStore: any InstallationKeyIdStoring
    private let entropyManager: any RootEntropyManaging
    private let eraser: any LocalStateErasing
    private let logger: LoggerProtocol

    init(
        keyIdStore: any InstallationKeyIdStoring,
        entropyManager: any RootEntropyManaging,
        eraser: any LocalStateErasing,
        logger: LoggerProtocol
    ) {
        self.keyIdStore = keyIdStore
        self.entropyManager = entropyManager
        self.eraser = eraser
        self.logger = logger
    }

    func migrate() throws {
        guard keyIdStore.getInstallationKeyId() != nil else {
            return
        }

        guard try !entropyManager.hasRootEntropy() else {
            return
        }

        logger
            .warning(
                "Installation key id found without root entropy: erasing the wallet state a device backup restored"
            )
        eraser.eraseUserState()
    }
}
