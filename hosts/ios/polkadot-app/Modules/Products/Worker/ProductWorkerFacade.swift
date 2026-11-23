import Foundation
import Keystore_iOS
import Products

/// Owns the product-worker ref counter and its operations, and on the native
/// runtime keeps a worker alive for every persisted operation.
///
/// One instance is created and held by `ServiceCoordinator` (alive while the
/// main tab bar is), so every consumer (the chats screen, a product's full-page
/// screen, and host-api operations) ref-counts the same per-product worker.
///
/// The boot factory is wired in `init`: the caller passes a builder that gets
/// the operations service (the worker's own JS uses it) and returns the factory,
/// which breaks the manager/factory/operations cycle without a late-bound lock.
final class ProductWorkerFacade {
    let manager: ProductWorkerManaging
    let operations: ProductWorkerOperating

    private let reconciler: ProductWorkerOperationReconciler
    private let settingsManager: SettingsManagerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared,
        makeFactory: (ProductWorkerOperating) -> ProductWorkerFactory
    ) {
        let store = CoreDataProductOperationStore(storageFacade: storageFacade)
        let operations = ProductWorkerOperationService(store: store)
        let manager = ProductWorkerManager(factory: makeFactory(operations), logger: logger)

        self.operations = operations
        self.manager = manager
        self.settingsManager = settingsManager
        reconciler = ProductWorkerOperationReconciler(store: store, manager: manager, logger: logger)
    }

    /// Starts keeping workers alive for persisted operations. Call once at launch.
    ///
    /// The reconciler stays off on TrUAPI, since reviving an operation left from a
    /// native run would start the product in the native container beside its
    /// TrUAPI worker, where a product built for TrUAPI gets no answer. A chat bot
    /// that falls back to the native worker on TrUAPI therefore gets no keep-alive
    /// for an operation it opens.
    func setup() {
        guard !settingsManager.isTrUAPIRuntimeEnabled else { return }
        reconciler.start()
    }
}
