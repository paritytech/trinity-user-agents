import UIKit
import Keystore_iOS
import KeyDerivation
import EventCenter

final class BackupInteractor {
    // MARK: Properties

    weak var presenter: BackupInteractorOutputProtocol?
    private let cloudKeychain: SynchronizableKeychain
    private let entropyManager: RootEntropyManaging
    private let logger: LoggerProtocol
    private let eventCenter: EventCenterProtocol
    private let backupTag = SynchronizableKeychainTag.walletEntropy

    // MARK: Initial methods

    init(
        cloudKeychain: SynchronizableKeychain,
        entropyManager: RootEntropyManaging,
        logger: LoggerProtocol,
        eventCenter: EventCenterProtocol = EventCenter.shared
    ) {
        self.cloudKeychain = cloudKeychain
        self.entropyManager = entropyManager
        self.logger = logger
        self.eventCenter = eventCenter
    }

    // MARK: Private methods

    private func checkBackup() throws -> Bool {
        try cloudKeychain.checkKey(for: backupTag)
    }
}

// MARK: - BackupInteractorInputProtocol

extension BackupInteractor: BackupInteractorInputProtocol {
    func checkBackupStatus() {
        let icloudIsAvailable = cloudKeychain.isAvailable
        if !icloudIsAvailable {
            MainActor.assumeIsolated {
                presenter?.didReceiveBackupStatus(.cloudIsOff)
            }
        } else {
            do {
                let hasBackup = try checkBackup()
                MainActor.assumeIsolated {
                    presenter?.didReceiveBackupStatus(hasBackup ? .created : .notFound)
                }
            } catch {
                logger.debug(error.localizedDescription)
                MainActor.assumeIsolated {
                    presenter?.didReceiveBackupStatus(.cloudIsOff)
                }
            }
        }
    }

    func backupMnemonic() {
        do {
            let entropy = try entropyManager.fetchRootEntropy()
            try cloudKeychain.addKey(entropy, with: backupTag)
            if try checkBackup() {
                MainActor.assumeIsolated {
                    presenter?.didReceiveBackupComplete(with: .success(()))
                }
                eventCenter.notify(with: BackupStatusChanged())
            } else {
                MainActor.assumeIsolated {
                    presenter?.didReceiveBackupComplete(with: .failure(BackupInteractorError.failedCreateBackup))
                }
            }
        } catch {
            MainActor.assumeIsolated {
                presenter?.didReceiveBackupComplete(with: .failure(error))
            }
        }
    }
}
