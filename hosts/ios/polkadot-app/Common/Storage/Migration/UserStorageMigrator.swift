import CoreData
import Foundation

final class UserStorageMigrator {
    let modelDirectory: String
    let model: UserStorageVersion
    let storeURL: URL
    let fileManager: FileManager

    init(
        storeURL: URL,
        modelDirectory: String,
        model: UserStorageVersion,
        fileManager: FileManager
    ) {
        self.storeURL = storeURL
        self.model = model
        self.modelDirectory = modelDirectory
        self.fileManager = fileManager
    }
}

// MARK: - StorageMigrating

extension UserStorageMigrator: StorageMigrating {
    func requiresMigration() -> Bool {
        checkIfMigrationNeeded(
            to: UserStorageParams.modelVersion,
            storeURL: storeURL,
            fileManager: fileManager,
            modelDirectory: modelDirectory
        )
    }

    func performMigration() {
        let destinationVersion = UserStorageParams.modelVersion

        let mom = createManagedObjectModel(
            forResource: destinationVersion.rawValue,
            modelDirectory: modelDirectory
        )

        let psc = NSPersistentStoreCoordinator(managedObjectModel: mom)
        // Persistent history tracking is enabled on the live store
        // (UserDataStorageFacade). Opening it here without this option
        // makes Core Data treat the store as read-only, and the v23 → v24
        // lightweight migration then fails with SQLite's misleading
        // "attempt to write a readonly database".
        let options: [AnyHashable: Any] = [
            NSMigratePersistentStoresAutomaticallyOption: true,
            NSInferMappingModelAutomaticallyOption: true,
            NSPersistentHistoryTrackingKey: true
        ]

        do {
            try psc.addPersistentStore(
                ofType: NSSQLiteStoreType,
                configurationName: nil,
                at: storeURL,
                options: options
            )
        } catch {
            fatalError("Failed to migrate persistent store: \(error)")
        }
    }

    func migrate(_ completion: @escaping () -> Void) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.performMigration()

            DispatchQueue.main.async {
                completion()
            }
        }
    }
}
