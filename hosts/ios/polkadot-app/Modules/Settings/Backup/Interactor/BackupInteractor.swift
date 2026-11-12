import UIKit
import Keystore_iOS
import KeyDerivation
import EventCenter
import Coinage

final class BackupInteractor {
    // MARK: Properties

    weak var presenter: BackupInteractorOutputProtocol?
    private let cloudKeychain: SynchronizableKeychain
    private let entropyManager: RootEntropyManaging
    private let coinageService: CoinageServicing
    private let coinageBackupSyncService: CoinageBackupSyncServicing
    private let logger: LoggerProtocol
    private let eventCenter: EventCenterProtocol
    private let backupTag = SynchronizableKeychainTag.walletEntropy

    private var recoveredBalanceTask: Task<Void, Never>?
    private var recoveryStateTask: Task<Void, Never>?
    private var balanceTask: Task<Void, Never>?

    // MARK: Initial methods

    init(
        cloudKeychain: SynchronizableKeychain,
        entropyManager: RootEntropyManaging,
        coinageService: CoinageServicing,
        coinageBackupSyncService: CoinageBackupSyncServicing,
        logger: LoggerProtocol,
        eventCenter: EventCenterProtocol = EventCenter.shared
    ) {
        self.cloudKeychain = cloudKeychain
        self.entropyManager = entropyManager
        self.coinageService = coinageService
        self.coinageBackupSyncService = coinageBackupSyncService
        self.logger = logger
        self.eventCenter = eventCenter
    }

    deinit {
        recoveredBalanceTask?.cancel()
        recoveryStateTask?.cancel()
        balanceTask?.cancel()
    }

    // MARK: Private methods

    private func checkBackup() throws -> Bool {
        try cloudKeychain.checkKey(for: backupTag)
    }
}

private extension BackupInteractor {
    func subscribeToRecoveredBalance() {
        recoveredBalanceTask = Task { [weak presenter, coinageBackupSyncService, logger] in
            do {
                for try await shows in coinageBackupSyncService.showsRecoveredBalanceStream {
                    await presenter?.didReceive(showsRecoveredBalance: shows)
                }
            } catch {
                logger.error("Restored balance stream failed: \(error)")
            }
        }
    }

    func subscribeToRecoveryState() {
        recoveryStateTask = Task { [weak presenter, coinageBackupSyncService, logger] in
            do {
                for try await inProgress in coinageBackupSyncService.isRecoveryInProgressStream {
                    await presenter?.didReceive(isRecoveryInProgress: inProgress)
                }
            } catch {
                logger.error("Recovery state stream failed: \(error)")
            }
        }
    }

    func subscribeToBalance() {
        balanceTask = Task { [weak presenter, coinageService, logger] in
            do {
                let balanceService = try await coinageService.coinageBalanceService()
                let context = balanceService.denominationContext
                for try await summary in balanceService.summaryStream {
                    let balance = summary.balance
                    await presenter?.didReceive(
                        coinageAmounts: CoinageAmounts(
                            total: context.decimal(fromPlanks: balance.total),
                            availableNow: context.decimal(fromPlanks: balance.ready),
                            gainingPrivacy: context.decimal(fromPlanks: balance.clearing)
                        )
                    )
                }
            } catch {
                logger.error("Balance stream failed: \(error)")
            }
        }
    }
}

// MARK: - BackupInteractorInputProtocol

extension BackupInteractor: BackupInteractorInputProtocol {
    func setup() {
        subscribeToRecoveredBalance()
        subscribeToRecoveryState()
        subscribeToBalance()
    }

    func triggerRecovery() {
        coinageBackupSyncService.triggerRecovery()
    }

    func acknowledgeRecovery() {
        coinageBackupSyncService.acknowledgeRecovery()
    }

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
