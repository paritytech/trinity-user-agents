import CoreData
import Foundation
import Operation_iOS
import OperationExt
import StructuredConcurrency
import Testing

@testable import polkadot_app

/// The snapshot subscriber must map only the rows the fetched results controller reports as changed.
/// Every case asserts the delivered snapshot and the exact number of mapper calls so far.
final class CoreDataSnapshotSubscriberTests {
    private struct TestError: Error {}

    private let facade = UserDataStorageTestFacade()
    private let deliveries = DeliveryLog<Chat.Contact>()
    private let mapper = CountingCoreDataMapper(ChatContactMapper())
    private var subscriber: CoreDataSnapshotSubscriber<Chat.Contact, CDChatContact>?

    private var contactRepository: AnyDataProviderRepository<Chat.Contact> {
        facade.makeRepo(mapper: ChatContactMapper())
    }

    @Test(.timeLimit(.minutes(1)))
    func initialSnapshotMapsEveryRowOnce() async throws {
        try await seed(count: 10)

        let initial = try await start()

        #expect(initial.map(\.username) == (0 ..< 10).map { "contact\($0)" })
        #expect(mapper.transformCount == 10)
    }

    @Test(.timeLimit(.minutes(1)))
    func updateRemapsOnlyTheChangedRow() async throws {
        try await seed(count: 10)
        _ = try await start()

        var renamed = Self.contact(index: 3)
        renamed.username = "renamed"
        try await contactRepository.saveOperation({ [renamed] }, { [] }).asyncExecute()

        let delivered = await deliveries.next()
        #expect(delivered.count == 10)
        #expect(delivered[3].username == "renamed")
        #expect(mapper.transformCount == 11)
    }

    @Test(.timeLimit(.minutes(1)))
    func insertRemapsOnlyTheNewRow() async throws {
        try await seed(count: 10)
        _ = try await start()

        try await contactRepository.saveOperation({ [Self.contact(index: 10)] }, { [] }).asyncExecute()

        let delivered = await deliveries.next()
        #expect(delivered.map(\.username) == (0 ..< 11).map { "contact\($0)" })
        #expect(mapper.transformCount == 11)
    }

    @Test(.timeLimit(.minutes(1)))
    func deleteRemapsNothing() async throws {
        try await seed(count: 10)
        _ = try await start()

        try await contactRepository.saveOperation({ [] }, { [Self.contact(index: 4).identifier] }).asyncExecute()

        let delivered = await deliveries.next()
        #expect(delivered.count == 9)
        #expect(!delivered.contains { $0.username == "contact4" })
        #expect(mapper.transformCount == 10)
    }

    @Test(.timeLimit(.minutes(1)))
    func mixedChangeRemapsOnlyTouchedRows() async throws {
        try await seed(count: 10)
        _ = try await start()

        var first = Self.contact(index: 1)
        first.username = "first-renamed"
        var second = Self.contact(index: 2)
        second.username = "second-renamed"

        try await contactRepository.saveOperation(
            { [first, second, Self.contact(index: 10)] },
            { [Self.contact(index: 9).identifier] }
        ).asyncExecute()

        let delivered = await deliveries.next()
        #expect(delivered.count == 10)
        #expect(delivered.contains { $0.username == "first-renamed" })
        #expect(delivered.contains { $0.username == "contact10" })
        #expect(!delivered.contains { $0.username == "contact9" })
        #expect(mapper.transformCount == 13)
    }

    @Test(.timeLimit(.minutes(1)))
    func failingRowIsSkippedAndRetried() async throws {
        try await seed(count: 10)
        let failing = Self.contact(index: 5)
        let flaky = FlakyMapper(ChatContactMapper(), failingOnceFor: failing.identifier)

        let initial = try await start(mapper: AnyCoreDataMapper(flaky))
        #expect(initial.count == 9)
        #expect(!initial.contains { $0.username == "contact5" })

        var retried = failing
        retried.username = "contact5-retried"
        try await contactRepository.saveOperation({ [retried] }, { [] }).asyncExecute()

        let delivered = await deliveries.next()
        #expect(delivered.count == 10)
        #expect(delivered.contains { $0.username == "contact5-retried" })
    }

    /// In `.serial` mode the controller sits on the writer, so an insert is reported under a temporary object ID
    /// during the save. The row must still be delivered once its ID is permanent and another change lands.
    @Test(.timeLimit(.minutes(1)))
    func insertedRowStaysInLaterSnapshots() async throws {
        try await seed(count: 10)
        _ = try await start()

        try await contactRepository.saveOperation({ [Self.contact(index: 10)] }, { [] }).asyncExecute()
        let afterInsert = await deliveries.next()
        #expect(afterInsert.count == 11)

        var renamed = Self.contact(index: 0)
        renamed.username = "renamed"
        try await contactRepository.saveOperation({ [renamed] }, { [] }).asyncExecute()

        let afterRename = await deliveries.next()
        #expect(afterRename.count == 11)
        #expect(afterRename.contains { $0.username == "contact10" })
        #expect(afterRename.contains { $0.username == "renamed" })
        #expect(mapper.transformCount <= 13)
    }

    /// `subscribeSnapshot` builds its request with no sort descriptors; inserts must still be delivered.
    @Test(.timeLimit(.minutes(1)))
    func insertIsDeliveredWithoutSortDescriptors() async throws {
        try await seed(count: 10)
        _ = try await start(sorted: false)

        try await contactRepository.saveOperation({ [Self.contact(index: 10)] }, { [] }).asyncExecute()
        let afterInsert = await deliveries.next()
        #expect(afterInsert.count == 11)
        #expect(mapper.transformCount == 11)

        try await contactRepository.saveOperation({ [Self.contact(index: 11)] }, { [] }).asyncExecute()
        let afterSecondInsert = await deliveries.next()
        #expect(afterSecondInsert.count == 12)
    }

    /// Same as `insertedRowStaysInLaterSnapshots`, on a SQLite store: there an insert is reported under a
    /// temporary object ID during the save and becomes permanent afterwards.
    @Test(.timeLimit(.minutes(1)))
    func insertedRowStaysInLaterSnapshotsOnDisk() async throws {
        let disk = try DiskStore()
        defer { disk.tearDown() }
        let repository = disk.contactRepository
        let contacts = (0 ..< 10).map { Self.contact(index: $0) }
        try await repository.saveOperation({ contacts }, { [] }).asyncExecute()
        _ = try await start(service: disk.service, sorted: false)

        try await repository.saveOperation({ [Self.contact(index: 10)] }, { [] }).asyncExecute()
        let afterInsert = await deliveries.next()
        #expect(afterInsert.count == 11)

        try await repository.saveOperation({ [Self.contact(index: 11)] }, { [] }).asyncExecute()
        let afterSecondInsert = await deliveries.next()
        #expect(afterSecondInsert.count == 12)
        #expect(afterSecondInsert.contains { $0.username == "contact10" })
    }

    /// B3's shape: messages whose mapper links a chat and creates a content row on insert, observed through
    /// `subscribeSnapshot` (no sort descriptors). Two inserts must yield snapshots of n+1 and n+2 rows.
    @Test(.timeLimit(.minutes(1)), arguments: [false, true])
    func insertedMessagesStayInLaterSnapshots(onDisk: Bool) async throws {
        let disk = onDisk ? try DiskStore() : nil
        defer { disk?.tearDown() }
        let service: CoreDataServiceProtocol = disk?.service ?? facade.databaseService

        let contact = Self.contact(index: 0)
        try await Self.repository(ChatContactMapper(), on: service).saveOperation({ [contact] }, { [] }).asyncExecute()
        try await Self.repository(ChatModelMapper(), on: service)
            .saveOperation({ [.newChatWithContact(contact)] }, { [] }).asyncExecute()
        let messages = Self.repository(ChatMessageEntityMapper(), on: service)
        try await messages.saveOperation({ (0 ..< 5).map { Self.message(index: $0, contact: contact) } }, { [] })
            .asyncExecute()

        let stream = service.subscribeSnapshot(mapper: AnyCoreDataMapper(ChatMessageEntityMapper()))
        let sizes = DeliveryLog<Int>()
        let consumer = Task {
            for try await models in stream {
                sizes.append([models.count])
            }
        }
        defer { consumer.cancel() }
        #expect(await sizes.next() == [5])

        try await messages.saveOperation({ [Self.message(index: 5, contact: contact)] }, { [] }).asyncExecute()
        #expect(await sizes.next() == [6])

        try await messages.saveOperation({ [Self.message(index: 6, contact: contact)] }, { [] }).asyncExecute()
        #expect(await sizes.next() == [7])
    }

    /// The pattern `CoinageTxRowObserver` relies on: a row is touched (will/didChangeValue) without any value
    /// change, so derived-state subscribers refresh exactly that row.
    @Test(.timeLimit(.minutes(1)))
    func touchedParentRowIsRemappedAlone() async throws {
        try await seed(count: 10)
        _ = try await start()

        let touched = Self.contact(index: 6)
        try await facade.databaseService.performWrite { context in
            let request = NSFetchRequest<CDChatContact>(entityName: String(describing: CDChatContact.self))
            request.predicate = NSPredicate(
                format: "%K == %@",
                #keyPath(CDChatContact.identifier),
                touched.identifier
            )
            guard let row = try context.fetch(request).first else {
                throw TestError()
            }
            row.willChangeValue(forKey: #keyPath(CDChatContact.devices))
            row.didChangeValue(forKey: #keyPath(CDChatContact.devices))
        }

        let delivered = await deliveries.next()
        #expect(delivered.count == 10)
        #expect(mapper.transformCount == 11)
    }
}

private extension CoreDataSnapshotSubscriberTests {
    static func contact(index: Int) -> Chat.Contact {
        var accountId = Data(repeating: 0xAB, count: 32)
        withUnsafeBytes(of: UInt32(index).bigEndian) { accountId.replaceSubrange(0 ..< 4, with: $0) }

        return Chat.Contact(
            accountId: accountId,
            username: "contact\(index)",
            publicKey: Data(repeating: UInt8(index), count: 32),
            pin: nil,
            pushId: nil,
            pushToken: nil,
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: .init(signKeyId: "sign-\(index)", encryptionKeyId: "encrypt-\(index)"),
            imageData: nil,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
    }

    static func repository<M: CoreDataMapperProtocol>(
        _ mapper: M,
        on service: CoreDataServiceProtocol
    ) -> AnyDataProviderRepository<M.DataProviderModel>
        where M.DataProviderModel: Identifiable, M.CoreDataEntity: NSManagedObject {
        AnyDataProviderRepository(
            CoreDataRepository(
                databaseService: service,
                mapper: AnyCoreDataMapper(mapper),
                filter: nil,
                sortDescriptors: []
            )
        )
    }

    static func message(index: Int, contact: Chat.Contact) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: "message-\(index)",
            chatId: .person(contact.accountId),
            origin: .contact(contact.accountId),
            creationSource: .localDevice,
            status: .incoming(.seen),
            timestamp: UInt64(index),
            content: .text("message \(index)"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func seed(count: Int) async throws {
        let contacts = (0 ..< count).map { Self.contact(index: $0) }
        try await contactRepository.saveOperation({ contacts }, { [] }).asyncExecute()
    }

    /// Starts a subscriber sorted by identifier and returns its first delivery.
    func start(
        mapper: AnyCoreDataMapper<Chat.Contact, CDChatContact>? = nil,
        service: CoreDataServiceProtocol? = nil,
        sorted: Bool = true
    ) async throws -> [Chat.Contact] {
        let request = NSFetchRequest<CDChatContact>(entityName: String(describing: CDChatContact.self))
        request.sortDescriptors = sorted
            ? [NSSortDescriptor(key: #keyPath(CDChatContact.identifier), ascending: true)]
            : []

        let deliveries = deliveries
        let subscriber = CoreDataSnapshotSubscriber(
            service: service ?? facade.databaseService,
            mapper: mapper ?? AnyCoreDataMapper(self.mapper),
            fetchRequest: request,
            callbackQueue: DispatchQueue(label: "io.polkadot.tests.snapshot"),
            onUpdate: { deliveries.append($0) }
        )
        self.subscriber = subscriber
        subscriber.start()

        return await deliveries.next()
    }
}

/// Collects deliveries in order and lets a test await the next one.
private final class DeliveryLog<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var pending: [[T]] = []
    private var waiters: [CheckedContinuation<[T], Never>] = []

    func append(_ models: [T]) {
        lock.lock()
        if !waiters.isEmpty {
            let waiter = waiters.removeFirst()
            lock.unlock()
            waiter.resume(returning: models)
        } else {
            pending.append(models)
            lock.unlock()
        }
    }

    func next() async -> [T] {
        await withCheckedContinuation { continuation in
            lock.lock()
            if !pending.isEmpty {
                let models = pending.removeFirst()
                lock.unlock()
                continuation.resume(returning: models)
            } else {
                waiters.append(continuation)
                lock.unlock()
            }
        }
    }
}

/// Throws once for one identifier, then behaves like the wrapped mapper.
private final class FlakyMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = Chat.Contact
    typealias CoreDataEntity = CDChatContact

    private struct FlakyError: Error {}

    private let base: ChatContactMapper
    private let failingIdentifier: String
    private var hasFailed = false

    init(_ base: ChatContactMapper, failingOnceFor identifier: String) {
        self.base = base
        failingIdentifier = identifier
    }

    var entityIdentifierFieldName: String {
        base.entityIdentifierFieldName
    }

    func transform(entity: CDChatContact) throws -> Chat.Contact {
        if !hasFailed, entity.identifier == failingIdentifier {
            hasFailed = true
            throw FlakyError()
        }

        return try base.transform(entity: entity)
    }

    func populate(entity: CDChatContact, from model: Chat.Contact, using context: NSManagedObjectContext) throws {
        try base.populate(entity: entity, from: model, using: context)
    }
}

/// A SQLite-backed `CoreDataService` in a temp directory, serial mode, history tracking on as in production.
private final class DiskStore {
    let service: CoreDataService
    private let directory: URL
    private let suiteName = "SnapshotSubscriberTests.\(UUID().uuidString)"

    var contactRepository: AnyDataProviderRepository<Chat.Contact> {
        AnyDataProviderRepository(
            CoreDataRepository(
                databaseService: service,
                mapper: AnyCoreDataMapper(ChatContactMapper()),
                filter: nil,
                sortDescriptors: []
            )
        )
    }

    init() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("SnapshotSubscriberTests.\(UUID().uuidString)", isDirectory: true)

        let modelName = UserStorageParams.modelVersion.rawValue
        let subdirectory = UserStorageParams.modelDirectory
        let modelURL = Bundle.main.url(forResource: modelName, withExtension: "omo", subdirectory: subdirectory)
            ?? Bundle.main.url(forResource: modelName, withExtension: "mom", subdirectory: subdirectory)

        guard let modelURL else {
            throw CoreDataServiceError.modelInitializationFailed
        }

        let tracking = CoreDataHistoryTrackingSettings(
            transactionAuthor: "test.app",
            targets: ["test.app", "test.extension"],
            sharedContainerName: suiteName
        )
        let settings = CoreDataPersistentSettings(
            databaseDirectory: directory,
            databaseName: "UserDataModel.sqlite",
            incompatibleModelStrategy: .removeStore,
            excludeFromiCloudBackup: true,
            historyTracking: tracking
        )
        service = CoreDataService(
            configuration: CoreDataServiceConfiguration(
                modelURL: modelURL,
                storageType: .persistent(settings: settings)
            )
        )
    }

    func tearDown() {
        try? service.close()
        try? FileManager.default.removeItem(at: directory)
        UserDefaults().removePersistentDomain(forName: suiteName)
    }
}
