import Foundation
import CoreData
import Operation_iOS
import OperationExt
import Products

protocol ProductPermissionDataProviderMaking {
    func subscribePermissionGrantsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([ProductPermissionGrant]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject
}

final class ProductPermissionDataProviderFactory {
    private let storageFacade: StorageFacadeProtocol
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.logger = logger
    }
}

extension ProductPermissionDataProviderFactory: ProductPermissionDataProviderMaking {
    func subscribePermissionGrantsSnapshot(
        for predicate: NSPredicate?,
        deliverOn queue: DispatchQueue,
        update: @escaping ([ProductPermissionGrant]) -> Void,
        failure: @escaping (Error) -> Void
    ) -> AnyObject {
        let request: NSFetchRequest<CDProductPermissionGrant> = CDProductPermissionGrant.fetchRequest()
        request.predicate = predicate
        request.sortDescriptors = [
            NSSortDescriptor(
                key: #keyPath(CDProductPermissionGrant.productId),
                ascending: true,
                selector: #selector(NSString.localizedCaseInsensitiveCompare)
            ),
            NSSortDescriptor(
                key: #keyPath(CDProductPermissionGrant.permissionType),
                ascending: true
            ),
            NSSortDescriptor(
                key: #keyPath(CDProductPermissionGrant.permissionKey),
                ascending: true
            )
        ]

        let mapper = ProductPermissionGrantMapper()
        let subscriber = CoreDataSnapshotSubscriber<ProductPermissionGrant, CDProductPermissionGrant>(
            service: storageFacade.databaseService,
            mapper: AnyCoreDataMapper(mapper),
            fetchRequest: request,
            callbackQueue: queue,
            logger: logger,
            transform: { $0 },
            onUpdate: update,
            onError: failure
        )

        subscriber.start()
        return subscriber
    }
}
