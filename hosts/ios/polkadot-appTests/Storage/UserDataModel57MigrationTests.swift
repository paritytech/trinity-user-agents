import CoreData
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// `UserDataModel57` keeps the contact and the last message of a room of the chat product, and drops what models 55
/// and 56 kept for the features that the app does not have: the avatar, the description, the text preview, the
/// mute, the archive and the personas of a room, and the search text and the author of a message.
final class UserDataModel57MigrationTests {
    private let directory = URL.temporaryDirectory.appending(path: "model57-\(UUID().uuidString)")
    private let roomChatId = Chat.Id.chatExtension("product.dot", roomId: "lobby")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func aStoreOfModel56KeepsItsRoomsAndDropsTheRemovedAttributes() throws {
        let storeURL = try makeStoreURL()
        try writeV56Room(to: storeURL)

        let migrator = migrator(storeURL: storeURL)
        #expect(migrator.requiresMigration())

        migrator.performMigration()

        #expect(!migrator.requiresMigration())
        let room = try readFirst("CDChatRoomMetadata", from: storeURL)
        #expect(room["name"] as? String == "Lobby")
        #expect(room["config"] as? Data == Data([1, 0, 1]))
        #expect(room["contact"] is NSNull)
        #expect(room["previewMessage"] is NSNull)
        #expect(room["previewUnseenReaction"] as? Bool == false)
        #expect(room["mutedUntil"] == nil)
        #expect(room["previewText"] == nil)

        let message = try readFirst("CDChatMessage", from: storeURL)
        #expect(message["messageId"] as? String == "m1")
        #expect(message["searchableText"] == nil)
    }

    @Test func aStoreOfModel54MigratesToModel57() throws {
        let storeURL = try makeStoreURL()
        try write(.version54, to: storeURL) { context in
            let metadata = NSEntityDescription.insertNewObject(forEntityName: "CDChatRoomMetadata", into: context)
            metadata.setValue("lobby", forKey: "chatRelativeId")
            metadata.setValue("Lobby", forKey: "name")
        }

        migrator(storeURL: storeURL).performMigration()

        let room = try readFirst("CDChatRoomMetadata", from: storeURL)
        #expect(room["name"] as? String == "Lobby")
        #expect(room["configGeneration"] as? Int64 == 0)
        #expect(room["contact"] is NSNull)
    }

    /// The facade opens the current model, so the app reads the contact through its mappers.
    @Test func theCurrentModelKeepsTheContactOfARoom() async throws {
        #expect(UserStorageVersion.version56.nextVersion() == .version57)

        let facade = UserDataStorageTestFacade()
        let contact = Chat.RoomContact(account: Data(repeating: 4, count: 32), username: "alice", state: .outgoing)
        let chat = Chat.LocalModel.newChatWithRoom(
            extensionId: "product.dot",
            roomId: "lobby",
            roomMetadata: Chat.RoomMetadata(chatRelativeId: "lobby", name: "Lobby", icon: nil)
        )
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .saveOperation({ [chat] }, { [] })
            .asyncExecute()
        try await facade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatRoomDeclarationUpdateMapper<Chat.RoomContactUpdate>())
        )
        .saveOperation({ [Chat.RoomContactUpdate(chatId: self.roomChatId, contact: contact)] }, { [] })
        .asyncExecute()

        let stored = try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .fetchOperation(by: { self.roomChatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
        #expect(stored?.roomContact == contact)
        #expect(stored?.roomDeclarations?.features == .hostDefault)
    }
}

private extension UserDataModel57MigrationTests {
    func makeStoreURL() throws -> URL {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appending(path: UserStorageParams.databaseName)
    }

    func migrator(storeURL: URL) -> UserStorageMigrator {
        UserStorageMigrator(
            storeURL: storeURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: UserStorageParams.modelVersion,
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

    /// A product room of model 56 with its declarations, a mute, a persona and one message.
    func writeV56Room(to storeURL: URL) throws {
        let chatId = roomChatId.rawRepresentation
        try write(.version56, to: storeURL) { context in
            let chat = NSEntityDescription.insertNewObject(forEntityName: "CDChat", into: context)
            chat.setValue(chatId, forKey: "identifier")
            chat.setValue(Int16(1), forKey: "chatType")
            chat.setValue("product.dot", forKey: "chatTypeContext")

            let metadata = NSEntityDescription.insertNewObject(forEntityName: "CDChatRoomMetadata", into: context)
            metadata.setValue("lobby", forKey: "chatRelativeId")
            metadata.setValue("Lobby", forKey: "name")
            metadata.setValue("Your turn", forKey: "previewText")
            metadata.setValue(Int64.max, forKey: "mutedUntil")
            metadata.setValue(Data([1, 0, 1]), forKey: "config")
            chat.setValue(metadata, forKey: "roomMetadata")

            let persona = NSEntityDescription.insertNewObject(forEntityName: "CDProductChatPersona", into: context)
            persona.setValue("product.dot", forKey: "productId")

            let message = NSEntityDescription.insertNewObject(forEntityName: "CDChatMessage", into: context)
            message.setValue("m1", forKey: "messageId")
            message.setValue("hello", forKey: "searchableText")
            message.setValue(chat, forKey: "chat")
        }
    }

    /// Reads the attributes inside the context: a managed object has no values once its context is gone.
    func readFirst(_ entityName: String, from storeURL: URL) throws -> [String: Any] {
        // The migrator takes a store to the current model, through 57.
        let coordinator = try openStore(UserStorageParams.modelVersion, storeURL: storeURL)
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator

        return try context.performAndWait {
            let request = NSFetchRequest<NSManagedObject>(entityName: entityName)
            let row = try #require(try context.fetch(request).first)
            let keys = Array(row.entity.attributesByName.keys)
            return row.dictionaryWithValues(forKeys: keys)
        }
    }
}
