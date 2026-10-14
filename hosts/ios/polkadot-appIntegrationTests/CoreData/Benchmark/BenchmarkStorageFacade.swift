import CoreData
import Foundation
import Operation_iOS

@testable import polkadot_app

enum BenchmarkError: Error {
    case modelNotFound
    case entityInsertFailed(String)
}

/// SQLite on disk in a per-instance temp directory, the production `UserDataModel`, history tracking on.
/// In-memory stores hide the coordinator lock and WAL, which is exactly what the benchmarks measure.
final class BenchmarkStorageFacade: StorageFacadeProtocol {
    static let appAuthor = "bench.app"
    static let extensionAuthor = "bench.nse"

    let databaseService: CoreDataServiceProtocol
    let variant: StackVariant
    let databaseDirectory: URL
    let suiteName: String

    private let ownsResources: Bool

    /// - Parameters:
    ///   - sharing: an existing facade whose SQLite file and timestamp suite this instance joins as a
    ///     second transaction author (the NSE shape). The owner cleans up; this one only closes.
    init(
        variant: StackVariant,
        author: String = BenchmarkStorageFacade.appAuthor,
        sharing: BenchmarkStorageFacade? = nil
    ) throws {
        self.variant = variant

        if let sharing {
            databaseDirectory = sharing.databaseDirectory
            suiteName = sharing.suiteName
            ownsResources = false
        } else {
            let name = "CoreDataBench.\(UUID().uuidString)"
            databaseDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(name, isDirectory: true)
            suiteName = "io.polkadot.bench.\(UUID().uuidString)"
            ownsResources = true
        }

        let tracking = CoreDataHistoryTrackingSettings(
            transactionAuthor: author,
            targets: [Self.appAuthor, Self.extensionAuthor],
            sharedContainerName: suiteName
        )

        let settings = CoreDataPersistentSettings(
            databaseDirectory: databaseDirectory,
            databaseName: "UserDataModel.sqlite",
            incompatibleModelStrategy: .removeStore,
            excludeFromiCloudBackup: true,
            historyTracking: tracking
        )

        let configuration = try CoreDataServiceConfiguration(
            modelURL: Self.modelURL(),
            storageType: .persistent(settings: settings),
            concurrencyMode: variant.concurrencyMode
        )

        databaseService = CoreDataService(configuration: configuration)
    }

    deinit {
        try? databaseService.close()

        guard ownsResources else {
            return
        }

        try? FileManager.default.removeItem(at: databaseDirectory)
        UserDefaults().removePersistentDomain(forName: suiteName)
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

    func makeRepo<M: CoreDataMapperProtocol>(
        mapper: M
    ) -> AnyDataProviderRepository<M.DataProviderModel>
        where M.DataProviderModel: Identifiable, M.CoreDataEntity: NSManagedObject {
        AnyDataProviderRepository(
            createRepository(filter: nil, sortDescriptors: [], mapper: AnyCoreDataMapper(mapper))
        )
    }

    /// The compiled production model from the test host bundle; shared with the topology spike.
    static func modelURL() throws -> URL {
        let modelName = UserStorageParams.modelVersion.rawValue
        let subdirectory = UserStorageParams.modelDirectory
        let bundle = Bundle.main

        let omoURL = bundle.url(forResource: modelName, withExtension: "omo", subdirectory: subdirectory)
        let momURL = bundle.url(forResource: modelName, withExtension: "mom", subdirectory: subdirectory)

        guard let url = omoURL ?? momURL else {
            throw BenchmarkError.modelNotFound
        }

        return url
    }
}
