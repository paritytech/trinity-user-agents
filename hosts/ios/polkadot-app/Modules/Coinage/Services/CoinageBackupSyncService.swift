import AsyncExtensions
import Coinage
import Foundation

// MARK: - Protocol

protocol CoinageBackupSyncServicing {
    /// Whether the restored-balance card is shown: a recovery found a balance the user has not
    /// confirmed yet. Emits the current value first.
    var showsRecoveredBalanceStream: AnyAsyncSequence<Bool> { get }

    /// Whether a scan is running; the card's "Update" shows progress while it is. Emits the current
    /// value first.
    var isRecoveryInProgressStream: AnyAsyncSequence<Bool> { get }

    /// Another look for balance under previous installations.
    func triggerRecovery()

    /// The user accepted the recovered balance and the card closes.
    func acknowledgeRecovery()
}

// MARK: - Implementation

/// Bridges the wallet's recovery progress to the wallet card.
final class CoinageBackupSyncService: CoinageBackupSyncServicing, @unchecked Sendable {
    private let coinageService: any CoinageServicing

    init(coinageService: any CoinageServicing) {
        self.coinageService = coinageService
    }

    var showsRecoveredBalanceStream: AnyAsyncSequence<Bool> {
        coinageService
            .subscribeBackupProgress()
            .map(\.awaitsAcknowledgement)
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }

    var isRecoveryInProgressStream: AnyAsyncSequence<Bool> {
        coinageService
            .subscribeBackupProgress()
            .map(\.isInProgress)
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }

    func triggerRecovery() {
        Task { [coinageService] in
            await coinageService.deepSearchBackup()
        }
    }

    func acknowledgeRecovery() {
        Task { [coinageService] in
            await coinageService.markBackupAsCompleted()
        }
    }
}
