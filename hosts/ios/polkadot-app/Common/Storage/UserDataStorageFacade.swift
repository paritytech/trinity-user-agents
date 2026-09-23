import CoreData
import Foundation
import Operation_iOS

enum UserStorageParams {
    ///  Controls which version of the UserDataModel to use
    ///  and also allows to understand how to do migration.
    ///  If there are changes that need to be applied to the model
    ///  go ahead with the following steps:
    ///  - create new version of the UserDataModel (for example, MultiassetUserDataModel5);
    ///  - add new case to UserStorageVersion and set associated value to the data model version name;
    ///  - add transition to UserStorageVersion.nextVersion;
    ///  - if lightweight migration is not an option then add NSEntityMigrationPolicy
    ///  and implement migration policy;
    ///  - update mappings between CoreData Entities and App Models;
    ///  - switch version of UserStorageParams.modelVersion;
    static let modelVersion: UserStorageVersion = .version49
    static let modelDirectory: String = "UserDataModel.momd"
    static let databaseName = "UserDataModel_v3.sqlite"

    static let sharedStorageDirectoryURL: URL = SharedContainerGroup
        .containerURL
        .appendingPathComponent("CoreData")

    static var storageURL: URL {
        sharedStorageDirectoryURL.appendingPathComponent(databaseName)
    }

    /// Suffix the extension appends to the bundle root for its own identifier.
    static let notificationServiceExtensionSuffix = ".NotificationServiceExtension"

    /// Every transaction author that writes to the shared store, the same list in every process:
    /// CoreDataHistoryCleaner needs a timestamp for EVERY target before it deletes anything, and each
    /// process writes its timestamp under its own bundle identifier.
    /// Bundle.main is unusable here: in the NSE process it resolves to the extension.
    static var historyTrackingTargets: [String] {
        let bundleRoot = AppConfig.Brand.bundleRoot

        return [
            bundleRoot,
            bundleRoot + notificationServiceExtensionSuffix
        ]
    }
}

class UserDataStorageFacade: StorageFacadeProtocol {
    static let shared = UserDataStorageFacade()

    let databaseService: CoreDataServiceProtocol

    private init() {
        let modelName = UserStorageParams.modelVersion.rawValue
        let bundle = Bundle.main

        let omoURL = bundle.url(
            forResource: modelName,
            withExtension: "omo",
            subdirectory: UserStorageParams.modelDirectory
        )

        let momURL = bundle.url(
            forResource: modelName,
            withExtension: "mom",
            subdirectory: UserStorageParams.modelDirectory
        )

        let modelURL = omoURL ?? momURL

        let historyTracking = CoreDataHistoryTrackingSettings(
            transactionAuthor: Bundle.main.bundleIdentifier ?? "",
            targets: UserStorageParams.historyTrackingTargets,
            sharedContainerName: SharedContainerGroup.name
        )

        let persistentSettings = CoreDataPersistentSettings(
            databaseDirectory: UserStorageParams.sharedStorageDirectoryURL,
            databaseName: UserStorageParams.databaseName,
            incompatibleModelStrategy: .ignore,
            historyTracking: historyTracking
        )

        let configuration = CoreDataServiceConfiguration(
            modelURL: modelURL!,
            storageType: .persistent(settings: persistentSettings),
            concurrencyMode: CoreDataConcurrencyPolicy.forCurrentTarget,
            logger: Logger.shared
        )

        databaseService = CoreDataService(configuration: configuration)
    }

    func createRepository<T, U>(
        filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor],
        mapper: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U> where T: Identifiable, U: NSManagedObject {
        CoreDataRepository(
            databaseService: databaseService,
            mapper: mapper,
            filter: filter,
            sortDescriptors: sortDescriptors
        )
    }
}
