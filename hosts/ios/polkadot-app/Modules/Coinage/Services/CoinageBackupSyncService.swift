import AsyncExtensions
import Coinage
import CoreData
import Foundation
import Operation_iOS

// MARK: - Protocol

protocol CoinageBackupSyncServicing {
    /// Whether the restored-balance card is shown: a previous installation whose first scan completed
    /// and whose balance the user has not confirmed yet. Emits the current value first.
    var showsRecoveredBalanceStream: AnyAsyncSequence<Bool> { get }

    /// Whether a scan is running; the card's "Update" shows progress while it is. Emits the current
    /// value first.
    var isRecoveryInProgressStream: AnyAsyncSequence<Bool> { get }

    /// Another look for balance under previous installations.
    func triggerRecovery()

    /// The user accepted the recovered balance: every installation known now is confirmed and the card
    /// closes until a later scan records a new one.
    func acknowledgeRecovery()
}

// MARK: - Implementation

/// Bridges Coinage's backup recovery to the wallet card. Visibility comes from the installation rows
/// themselves, so a running or failed deep search cannot hide a balance already offered; progress only
/// drives the button.
final class CoinageBackupSyncService: CoinageBackupSyncServicing, @unchecked Sendable {
    private let coinageService: any CoinageServicing
    private let storageFacade: StorageFacadeProtocol

    init(coinageService: any CoinageServicing, storageFacade: StorageFacadeProtocol) {
        self.coinageService = coinageService
        self.storageFacade = storageFacade
    }

    var showsRecoveredBalanceStream: AnyAsyncSequence<Bool> {
        storageFacade
            .subscribeSnapshot(
                mapper: AnyCoreDataMapper(RecoveredInstallationMapper()),
                filter: Self.awaitingConfirmation
            )
            .map { !$0.isEmpty }
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

private extension CoinageBackupSyncService {
    static var awaitingConfirmation: NSPredicate {
        NSPredicate(
            format: "%K == YES AND %K == NO",
            #keyPath(CDCoinageInstallation.initialScanCompleted),
            #keyPath(CDCoinageInstallation.isUserConfirmedCompletion)
        )
    }
}

// MARK: - Row observation

/// A previous installation the card is shown for; only its identity is needed.
struct RecoveredInstallation: Operation_iOS.Identifiable, Equatable {
    let identifier: String
}

final class RecoveredInstallationMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = RecoveredInstallation
    typealias CoreDataEntity = CDCoinageInstallation

    var entityIdentifierFieldName: String { #keyPath(CDCoinageInstallation.identifier) }

    func transform(entity: CDCoinageInstallation) throws -> RecoveredInstallation {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoinageInstallation.identifier))
        }
        return RecoveredInstallation(identifier: identifier)
    }

    func populate(
        entity: CDCoinageInstallation,
        from model: RecoveredInstallation,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
    }
}
