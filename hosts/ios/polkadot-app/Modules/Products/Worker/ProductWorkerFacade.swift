import Foundation
import Keystore_iOS
import Products

/// Owns the product-worker ref counter that chat falls back to while the TrUAPI workers are
/// not running.
///
/// One instance is created and held by `ServiceCoordinator` (alive while the
/// main tab bar is), so every consumer ref-counts the same per-product worker.
///
/// The boot factory is wired in `init`: the caller passes a builder that gets
/// the operations service (the worker's own JS uses it) and returns the factory,
/// which breaks the manager/factory/operations cycle without a late-bound lock.
final class ProductWorkerFacade {
    let manager: ProductWorkerManaging

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared,
        makeFactory: (ProductWorkerOperating) -> ProductWorkerFactory
    ) {
        let store = CoreDataProductOperationStore(storageFacade: storageFacade)
        let operations = ProductWorkerOperationService(store: store)
        manager = ProductWorkerManager(factory: makeFactory(operations), logger: logger)
    }
}
