import Foundation
import CoreData
import Operation_iOS
import SDKLogger

public enum CoreDataSnapshotSubscriberError: Error {
    case serviceError(Error)
    case missingContext
    case fetchFailed(Error)
}

/// Delivers the full, ordered snapshot of a fetch request on every change, mapping only the rows the
/// fetched results controller reports as changed: their cached models are invalidated and rebuilt at
/// delivery. The cache is keyed by permanent object ID and lives on the observer context's queue.
public final class CoreDataSnapshotSubscriber<Model: Identifiable, Entity: NSManagedObject>:
    NSObject,
    NSFetchedResultsControllerDelegate {
    private let service: CoreDataServiceProtocol
    private let mapper: AnyCoreDataMapper<Model, Entity>
    private let fetchRequest: NSFetchRequest<Entity>
    private let callbackQueue: DispatchQueue
    private let logger: SDKLoggerProtocol?
    private let transform: ([Model]) -> [Model]
    private let onUpdate: ([Model]) -> Void
    private let onError: ((Error) -> Void)?

    private var fetchController: NSFetchedResultsController<Entity>?
    private var cache: [NSManagedObjectID: Model] = [:]
    private var changedObjectIDs: Set<NSManagedObjectID> = []

    public init(
        service: CoreDataServiceProtocol,
        mapper: AnyCoreDataMapper<Model, Entity>,
        fetchRequest: NSFetchRequest<Entity>,
        callbackQueue: DispatchQueue,
        logger: SDKLoggerProtocol? = nil,
        transform: @escaping ([Model]) -> [Model] = { $0 },
        onUpdate: @escaping ([Model]) -> Void,
        onError: ((Error) -> Void)? = nil
    ) {
        self.service = service
        self.mapper = mapper
        self.fetchRequest = fetchRequest
        self.callbackQueue = callbackQueue
        self.logger = logger
        self.transform = transform
        self.onUpdate = onUpdate
        self.onError = onError
        super.init()
    }

    public func start() {
        service.performObserve { [weak self] context, error in
            guard let self else {
                return
            }

            if let error {
                logger?.error("fetchController start error: \(error)")
                notifyError(CoreDataSnapshotSubscriberError.serviceError(error))
                return
            }

            guard let context else {
                logger?.error("fetchController start error: context missed")
                notifyError(CoreDataSnapshotSubscriberError.missingContext)
                return
            }

            let fetchController = NSFetchedResultsController(
                fetchRequest: fetchRequest,
                managedObjectContext: context,
                sectionNameKeyPath: nil,
                cacheName: nil
            )
            fetchController.delegate = self

            do {
                try fetchController.performFetch()
                self.fetchController = fetchController
                cache.removeAll()
                deliverSnapshot()
            } catch {
                logger?.error("fetchController performFetch error: \(error)")
                notifyError(CoreDataSnapshotSubscriberError.fetchFailed(error))
            }
        }
    }

    // MARK: - NSFetchedResultsControllerDelegate

    public func controllerWillChangeContent(_: NSFetchedResultsController<NSFetchRequestResult>) {
        changedObjectIDs.removeAll()
    }

    public func controller(
        _: NSFetchedResultsController<NSFetchRequestResult>,
        didChange anObject: Any,
        at _: IndexPath?,
        for _: NSFetchedResultsChangeType,
        newIndexPath _: IndexPath?
    ) {
        guard let entity = anObject as? Entity else {
            return
        }

        changedObjectIDs.insert(entity.objectID)
    }

    public func controllerDidChangeContent(_: NSFetchedResultsController<NSFetchRequestResult>) {
        changedObjectIDs.forEach { cache.removeValue(forKey: $0) }
        changedObjectIDs.removeAll()
        deliverSnapshot()
    }
}

private extension CoreDataSnapshotSubscriber {
    /// Assembles the snapshot in the controller's order: cached models are reused, every other row is
    /// mapped now. A row whose object ID is still temporary (reported during the save that inserts it) is
    /// mapped but not cached, so it is looked up correctly once its permanent ID is assigned.
    func deliverSnapshot() {
        guard let objects = fetchController?.fetchedObjects else {
            return
        }

        let models = objects.compactMap { entity in
            cache[entity.objectID] ?? map(entity)
        }
        let processedModels = transform(models)

        callbackQueue.async { [onUpdate] in
            onUpdate(processedModels)
        }
    }

    func map(_ entity: Entity) -> Model? {
        do {
            let model = try mapper.transform(entity: entity)

            if !entity.objectID.isTemporaryID {
                cache[entity.objectID] = model
            }

            return model
        } catch {
            logger?.error("Map entity failed: \(error)")
            return nil
        }
    }

    func notifyError(_ error: Error) {
        callbackQueue.async { [onError] in
            onError?(error)
        }
    }
}
