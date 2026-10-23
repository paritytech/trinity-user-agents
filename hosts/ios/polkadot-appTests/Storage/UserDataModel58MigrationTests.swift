import CoreData
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// `UserDataModel58` adds the payments of the rooms of the chat product (`CDProductChatPayment`), which the host keeps
/// with the handoff of their coins. A store of model 57 keeps everything it holds.
final class UserDataModel58MigrationTests {
    private let directory = URL.temporaryDirectory.appending(path: "model58-\(UUID().uuidString)")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func aStoreOfModel57KeepsItsMessagesAndHasNoPayments() throws {
        let storeURL = try makeStoreURL()
        try write(.version57, to: storeURL) { context in
            let message = NSEntityDescription.insertNewObject(forEntityName: "CDChatMessage", into: context)
            message.setValue("m1", forKey: "messageId")
        }

        let migrator = migrator(storeURL: storeURL)
        #expect(migrator.requiresMigration())

        migrator.performMigration()

        #expect(!migrator.requiresMigration())
        #expect(try count("CDChatMessage", in: storeURL) == 1)
        #expect(try count("CDProductChatPayment", in: storeURL) == 0)
    }

    @Test func theCurrentModelIsModel58() async throws {
        #expect(UserStorageParams.modelVersion == .version58)
        #expect(UserStorageVersion.version57.nextVersion() == .version58)
        #expect(UserStorageVersion.version58.nextVersion() == nil)

        let store = ProductChatPaymentStore(storageFacade: UserDataStorageTestFacade())
        let payment = ProductChatPayment(
            messageId: "pay-1",
            productId: "chat.dot",
            roomId: "dm-7",
            incoming: false,
            timestamp: 5,
            totalValue: 10,
            coinKeys: [Data(repeating: 7, count: 64)],
            delivery: .sending,
            state: nil
        )
        try await store.saveOutgoing(payment) { _ in }
        try await store.updateOutgoing(messageId: "pay-1", state: .init(status: .sent))

        var expected = payment
        expected.state = .outgoing(.init(status: .sent))
        #expect(try await store.payments() == [expected])
    }
}

private extension UserDataModel58MigrationTests {
    func makeStoreURL() throws -> URL {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appending(path: UserStorageParams.databaseName)
    }

    func migrator(storeURL: URL) -> UserStorageMigrator {
        UserStorageMigrator(
            storeURL: storeURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: .version58,
            fileManager: .default
        )
    }

    func model(_ version: UserStorageVersion) throws -> NSManagedObjectModel {
        let bundle = Bundle.main
        let url = bundle.url(
            forResource: version.rawValue,
            withExtension: "omo",
            subdirectory: UserStorageParams.modelDirectory
        ) ?? bundle.url(
            forResource: version.rawValue,
            withExtension: "mom",
            subdirectory: UserStorageParams.modelDirectory
        )

        return try #require(url.flatMap(NSManagedObjectModel.init(contentsOf:)))
    }

    func openStore(_ version: UserStorageVersion, storeURL: URL) throws -> NSPersistentStoreCoordinator {
        let coordinator = try NSPersistentStoreCoordinator(managedObjectModel: model(version))
        try coordinator.addPersistentStore(
            ofType: NSSQLiteStoreType,
            configurationName: nil,
            at: storeURL,
            options: [NSPersistentHistoryTrackingKey: true]
        )
        return coordinator
    }

    func write(
        _ version: UserStorageVersion,
        to storeURL: URL,
        _ block: @escaping (NSManagedObjectContext) throws -> Void
    ) throws {
        let coordinator = try openStore(version, storeURL: storeURL)
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator

        try context.performAndWait {
            try block(context)
            try context.save()
        }

        for store in coordinator.persistentStores {
            try coordinator.remove(store)
        }
    }

    func count(_ entityName: String, in storeURL: URL) throws -> Int {
        let coordinator = try openStore(.version58, storeURL: storeURL)
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator

        return try context.performAndWait {
            try context.count(for: NSFetchRequest<NSManagedObject>(entityName: entityName))
        }
    }
}
