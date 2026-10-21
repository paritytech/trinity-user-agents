import CoreData
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// `UserDataModel56` adds the storage of the Chat Extension v2 additions, for every P10 item (07-LATER §4.4): the
/// features, the mute and the archive state of a room, the search text of a message and the personas of products.
final class UserDataModel56MigrationTests {
    private let directory = URL.temporaryDirectory.appending(path: "model56-\(UUID().uuidString)")
    private let roomChatId = Chat.Id.chatExtension("product.dot", roomId: "lobby")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func aStoreWithV2ProductRoomsMigratesWithItsDeclarationsAndNoAdditions() throws {
        let storeURL = try makeStoreURL()
        try writeV55Room(to: storeURL)

        let migrator = migrator(storeURL: storeURL)
        #expect(migrator.requiresMigration())

        migrator.performMigration()

        #expect(!migrator.requiresMigration())
        let room = try readFirst("CDChatRoomMetadata", from: storeURL)
        #expect(room["name"] as? String == "Lobby")
        #expect(room["previewText"] as? String == "Your turn")
        #expect(room["config"] as? Data == Data([1, 0, 0, 0, 0, 0, 0, 0]))
        #expect(room["features"] is NSNull)
        #expect(room["mutedUntil"] as? Int64 == 0)
        #expect(room["archived"] as? Bool == false)

        let message = try readFirst("CDChatMessage", from: storeURL)
        #expect(message["messageId"] as? String == "m1")
        #expect(message["searchableText"] is NSNull)
    }

    @Test func aStoreOfModel54MigratesToModel56() throws {
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
        #expect(room["mutedUntil"] as? Int64 == 0)
    }

    @Test func thePersonaEntityTakesRowsThatOutliveAChat() throws {
        let storeURL = try makeStoreURL()
        try writeV55Room(to: storeURL)
        migrator(storeURL: storeURL).performMigration()

        let coordinator = try openStore(.version56, storeURL: storeURL)
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator

        let counts = try context.performAndWait {
            let chat = try #require(try context.fetch(NSFetchRequest<NSManagedObject>(entityName: "CDChat")).first)
            let persona = NSEntityDescription.insertNewObject(forEntityName: "CDProductChatPersona", into: context)
            persona.setValue("product.dot", forKey: "productId")
            persona.setValue("helper", forKey: "botId")
            try context.save()

            context.delete(chat)
            try context.save()

            return try context.count(for: NSFetchRequest<NSManagedObject>(entityName: "CDProductChatPersona"))
        }

        #expect(counts == 1)
    }

    /// The facade opens the current model, so the app reads the new attributes through its mappers.
    @Test func theCurrentModelIsModel56() async throws {
        #expect(UserStorageParams.modelVersion == .version56)
        #expect(UserStorageVersion.version55.nextVersion() == .version56)
        #expect(UserStorageVersion.version56.nextVersion() == nil)

        let facade = UserDataStorageTestFacade()
        let state = Chat.RoomUserState(mutedUntil: 1_700_000_000_000, archived: true)
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
            mapper: AnyCoreDataMapper(ChatRoomDeclarationUpdateMapper<Chat.RoomUserStateUpdate>())
        )
        .saveOperation({ [Chat.RoomUserStateUpdate(chatId: self.roomChatId, state: state)] }, { [] })
        .asyncExecute()

        let stored = try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .fetchOperation(by: { self.roomChatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
        #expect(stored?.roomDeclarations?.userState == state)
        #expect(stored?.roomDeclarations?.features == .hostDefault)
    }
}

private extension UserDataModel56MigrationTests {
    func makeStoreURL() throws -> URL {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appending(path: UserStorageParams.databaseName)
    }

    func migrator(storeURL: URL) -> UserStorageMigrator {
        UserStorageMigrator(
            storeURL: storeURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: .version56,
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

    /// A product room of model 55 with its declarations and one message.
    func writeV55Room(to storeURL: URL) throws {
        let chatId = roomChatId.rawRepresentation
        try write(.version55, to: storeURL) { context in
            let chat = NSEntityDescription.insertNewObject(forEntityName: "CDChat", into: context)
            chat.setValue(chatId, forKey: "identifier")
            chat.setValue(Int16(1), forKey: "chatType")
            chat.setValue("product.dot", forKey: "chatTypeContext")

            let metadata = NSEntityDescription.insertNewObject(forEntityName: "CDChatRoomMetadata", into: context)
            metadata.setValue("lobby", forKey: "chatRelativeId")
            metadata.setValue("Lobby", forKey: "name")
            metadata.setValue("Your turn", forKey: "previewText")
            metadata.setValue(Data([1, 0, 0, 0, 0, 0, 0, 0]), forKey: "config")
            chat.setValue(metadata, forKey: "roomMetadata")

            let message = NSEntityDescription.insertNewObject(forEntityName: "CDChatMessage", into: context)
            message.setValue("m1", forKey: "messageId")
            message.setValue(chat, forKey: "chat")
        }
    }

    /// Reads the attributes inside the context: a managed object has no values once its context is gone.
    func readFirst(_ entityName: String, from storeURL: URL) throws -> [String: Any] {
        let coordinator = try openStore(.version56, storeURL: storeURL)
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
