import Foundation
import Foundation_iOS
import Operation_iOS
import ChainRegistry
import EventCenter
import SubstrateSdk

enum ChainRegistryFactory {
    ///  Creates chain registry with on-disk database manager. This function must be used by the application
    ///  by default.
    ///
    ///  - Returns: new instance conforming to `ChainRegistryProtocol`.

    static func createDefaultRegistry() -> ChainRegistryProtocol {
        let repositoryFacade = SubstrateDataStorageFacade.shared
        return createDefaultRegistry(from: repositoryFacade)
    }

    ///  Creates chain registry with provided database manager. This function must be used when
    ///  there is a need to override `createDefaultRegistry()` behavior that stores database on disk.
    ///  For example, in tests it is more conveinent to use in-memory database.
    ///
    ///  - Parameters:
    ///      - repositoryFacade: Database manager to use for chain registry.
    ///
    ///  - Returns: new instance conforming to `ChainRegistryProtocol`.
    static func createDefaultRegistry(
        from repositoryFacade: StorageFacadeProtocol
    ) -> ChainRegistryProtocol {
        let runtimeMetadataRepository: CoreDataRepository<RuntimeMetadataItem, CDRuntimeMetadataItem> =
            repositoryFacade.createRepository()

        let dataFetchOperationFactory = DataOperationFactory()

        let filesOperationFactory = createFilesOperationFactory()

        let runtimeSyncService = RuntimeSyncService(
            repository: AnyDataProviderRepository(runtimeMetadataRepository),
            runtimeFetchFactory: RuntimeFetchOperationFactory(operationQueue: OperationManagerFacade.runtimeSyncQueue),
            runtimeLocalMigrator: RuntimeLocalMigrator.createLatest(),
            filesOperationFactory: filesOperationFactory,
            dataOperationFactory: dataFetchOperationFactory,
            eventCenter: EventCenter.shared,
            operationQueue: OperationManagerFacade.runtimeSyncQueue,
            logger: Logger.shared
        )

        let runtimeProviderFactory = RuntimeProviderFactory(
            fileOperationFactory: filesOperationFactory,
            repository: AnyDataProviderRepository(runtimeMetadataRepository),
            dataOperationFactory: dataFetchOperationFactory,
            signedExtensionCodersFactory: CompoundTxExtensionFactory(),
            eventCenter: EventCenter.shared,
            operationQueue: OperationManagerFacade.runtimeSyncQueue,
            logger: Logger.shared
        )

        let runtimeProviderPool = RuntimeProviderPool(runtimeProviderFactory: runtimeProviderFactory)

        let connectionPool = ConnectionPool(
            connectionFactory: ConnectionFactory(
                apiKeysProvider: ConnectionApiKeysProvider.shared,
                logger: Logger.shared,
                operationQueue: OperationManagerFacade.runtimeSyncQueue,
                reachabilityManager: ReachabilityManager.shared
            ),
            applicationHandler: ApplicationHandler()
        )

        let mapper = ChainModelMapper()
        let chainRepository: CoreDataRepository<ChainModel, CDChain> =
            repositoryFacade.createRepository(mapper: AnyCoreDataMapper(mapper))

        let chainProvider = createChainProvider(from: repositoryFacade, chainRepository: chainRepository)

        let chainSyncService = ChainSyncService(
            remoteConfigManager: FirebaseFacade.shared,
            chainConverter: ChainModelConverter(),
            repository: AnyDataProviderRepository(chainRepository),
            eventCenter: EventCenter.shared,
            operationQueue: OperationManagerFacade.runtimeSyncQueue,
            logger: Logger.shared
        )

        let specVersionSubscriptionFactory = SpecVersionSubscriptionFactory(
            runtimeSyncService: runtimeSyncService,
            logger: Logger.shared
        )

        return ChainRegistry(
            runtimeProviderPool: runtimeProviderPool,
            connectionPool: connectionPool,
            chainSyncService: chainSyncService,
            runtimeSyncService: runtimeSyncService,
            commonTypesSyncService: nil,
            chainProvider: chainProvider,
            specVersionSubscriptionFactory: specVersionSubscriptionFactory,
            logger: Logger.shared
        )
    }

    private static func createFilesOperationFactory() -> RuntimeFilesOperationFactoryProtocol {
        let topDirectory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first ??
            FileManager.default.temporaryDirectory
        let runtimeDirectory = topDirectory.appendingPathComponent("runtime").path
        return RuntimeFilesOperationFactory(
            repository: FileRepository(),
            directoryPath: runtimeDirectory
        )
    }

    private static func createChainProvider(
        from repositoryFacade: StorageFacadeProtocol,
        chainRepository: CoreDataRepository<ChainModel, CDChain>
    ) -> StreamableProvider<ChainModel> {
        let chainObserver = CoreDataContextObservable(
            service: repositoryFacade.databaseService,
            mapper: chainRepository.dataMapper,
            predicate: { _ in true }
        )

        chainObserver.start { error in
            if let error {
                (Logger.shared as LoggerProtocol).error("Chain database observer unexpectedly failed: \(error)")
            }
        }

        return StreamableProvider(
            source: AnyStreamableSource(EmptyStreamableSource<ChainModel>()),
            repository: AnyDataProviderRepository(chainRepository),
            observable: AnyDataProviderRepositoryObservable(chainObserver),
            operationManager: OperationManager(operationQueue: OperationManagerFacade.sharedDefaultQueue)
        )
    }
}
