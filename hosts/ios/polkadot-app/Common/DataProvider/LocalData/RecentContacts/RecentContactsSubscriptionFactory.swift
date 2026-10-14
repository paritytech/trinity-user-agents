import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

protocol RecentContactsSubscriptionFactoryProtocol {
    func getAllRecentContacts() throws -> StreamableProvider<RecentContactModel>
    func getAllRecentContacts(for chainAssetID: ChainAssetId) throws -> StreamableProvider<RecentContactModel>
}

final class RecentContactsSubscriptionFactory: SubstrateLocalSubscriptionFactory,
    RecentContactsSubscriptionFactoryProtocol {
    static let shared = RecentContactsSubscriptionFactory(
        chainRegistry: ChainRegistryFacade.sharedRegistry,
        storageFacade: UserDataStorageFacade.shared,
        operationQueue: OperationManagerFacade.sharedDefaultQueue,
        logger: Logger.shared
    )

    func getAllRecentContacts() throws -> StreamableProvider<RecentContactModel> {
        let cacheKey = "all-recent-contacts"

        if let provider = getProvider(for: cacheKey) as? StreamableProvider<RecentContactModel> {
            return provider
        }

        let source = EmptyStreamableSource<RecentContactModel>()

        let mapper = RecentContactMapper()
        let repository = storageFacade.createRepository(mapper: AnyCoreDataMapper(mapper))

        let observable = CoreDataContextObservable(
            service: storageFacade.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            predicate: { _ in true }
        )

        observable.start { [weak self] error in
            if let error {
                self?.logger.error("Did receive error: \(error)")
            }
        }

        let provider = StreamableProvider(
            source: AnyStreamableSource(source),
            repository: AnyDataProviderRepository(repository),
            observable: AnyDataProviderRepositoryObservable(observable),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        saveProvider(provider, for: cacheKey)

        return provider
    }

    func getAllRecentContacts(for chainAssetID: ChainAssetId) throws -> StreamableProvider<RecentContactModel> {
        let cacheKey = chainAssetID.stringValue
        if let provider = getProvider(for: cacheKey) as? StreamableProvider<RecentContactModel> {
            return provider
        }

        let source = EmptyStreamableSource<RecentContactModel>()

        let mapper = RecentContactMapper()
        let filter = filter(for: chainAssetID)
        let repository = storageFacade.createRepository(
            filter: filter,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(mapper)
        )

        let observable = CoreDataContextObservable(
            service: storageFacade.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            predicate: { entity in
                entity.chainID == chainAssetID.chainId && entity.assetID == Int64(bitPattern: chainAssetID.assetId)
            }
        )

        observable.start { [weak self] error in
            if let error {
                self?.logger.error("Did receive error: \(error)")
            }
        }

        let provider = StreamableProvider(
            source: AnyStreamableSource(source),
            repository: AnyDataProviderRepository(repository),
            observable: AnyDataProviderRepositoryObservable(observable),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        saveProvider(provider, for: cacheKey)

        return provider
    }

    private func filter(for chainAssetID: ChainAssetId) -> NSPredicate {
        let chainIDPredicate = NSPredicate(
            format: "%K == %@",
            #keyPath(CDRecentContact.chainID),
            chainAssetID.chainId
        )

        let assetIDPredicate = NSPredicate(
            format: "%K == %d",
            #keyPath(CDRecentContact.assetID),
            Int64(bitPattern: chainAssetID.assetId)
        )

        return NSCompoundPredicate(andPredicateWithSubpredicates: [chainIDPredicate, assetIDPredicate])
    }
}
