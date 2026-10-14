import CoreData
import Operation_iOS

enum SubstrateStorageParams {
    static let databaseName = "SubstrateDataModel_v3.sqlite"
    static let modelDirectory: String = "SubstrateDataModel.momd"
    static let modelVersion: SubstrateStorageVersion = .version4

    static let sharedStorageDirectoryURL: URL = SharedContainerGroup
        .containerURL
        .appendingPathComponent("CoreData")

    static var storageURL: URL {
        sharedStorageDirectoryURL.appendingPathComponent(databaseName)
    }
}

class SubstrateDataStorageFacade: StorageFacadeProtocol {
    static let shared = SubstrateDataStorageFacade()

    let databaseService: CoreDataServiceProtocol

    private init() {
        let databaseName = SubstrateStorageParams.databaseName
        let modelName = SubstrateStorageParams.modelVersion.rawValue
        let bundle = Bundle.main

        let omoURL = bundle.url(
            forResource: modelName,
            withExtension: "omo",
            subdirectory: SubstrateStorageParams.modelDirectory
        )

        let momURL = bundle.url(
            forResource: modelName,
            withExtension: "mom",
            subdirectory: SubstrateStorageParams.modelDirectory
        )

        let modelURL = omoURL ?? momURL

        let persistentSettings = CoreDataPersistentSettings(
            databaseDirectory: SubstrateStorageParams.sharedStorageDirectoryURL,
            databaseName: databaseName,
            incompatibleModelStrategy: .ignore
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
