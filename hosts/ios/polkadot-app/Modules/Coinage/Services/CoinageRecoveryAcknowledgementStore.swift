import Coinage
import CoreData
import Foundation
@preconcurrency import Keystore_iOS
import Operation_iOS

/// Whether the user accepted the balance a recovery found, kept in `SettingsManager`.
///
/// Until the user answers once, it is what the app's own recovery left: accepted, unless an earlier
/// installation was scanned and its balance not yet confirmed.
final class CoinageRecoveryAcknowledgementStore: CoinageRecoveryAcknowledging, @unchecked Sendable {
    private let settingsManager: SettingsManagerProtocol
    private let databaseService: CoreDataServiceProtocol

    init(
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) {
        self.settingsManager = settingsManager
        databaseService = storageFacade.databaseService
    }

    var isAcknowledged: Bool {
        if let stored = settingsManager.bool(for: SettingsKey.coinageRecoveryAcknowledged.rawValue) {
            return stored
        }

        let awaitingConfirmation = try? databaseService.performReadAndWait { context in
            let request = NSFetchRequest<CDCoinageInstallation>(entityName: "CDCoinageInstallation")
            request.predicate = NSPredicate(
                format: "%K == YES AND %K == NO",
                #keyPath(CDCoinageInstallation.initialScanCompleted),
                #keyPath(CDCoinageInstallation.isUserConfirmedCompletion)
            )
            return try context.count(for: request) > 0
        }
        return awaitingConfirmation != true
    }

    func save(isAcknowledged: Bool) {
        settingsManager.set(value: isAcknowledged, for: .coinageRecoveryAcknowledged)
    }
}
