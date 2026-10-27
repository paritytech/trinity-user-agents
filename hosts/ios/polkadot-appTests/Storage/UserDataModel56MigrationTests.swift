import CoreData
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// `UserDataModel56` adds the declarations of a product room to `CDChatRoomMetadata`: the configuration and its
/// generation, the badge and the chat-list preview. A lightweight migration keeps every room and gives the new
/// attributes their defaults.
final class UserDataModel56MigrationTests {
    private let directory = URL.temporaryDirectory.appending(path: "model56-\(UUID().uuidString)")
    private let roomChatId = Chat.Id.chatExtension("product.dot", roomId: "lobby")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func aStoreOfModel55KeepsItsRoomsAndReadsNoDeclarations() throws {
        let storeURL = try makeStoreURL()
        try writeV55Data(to: storeURL)

        let migrator = migrator(storeURL: storeURL)
        #expect(migrator.requiresMigration())

        migrator.performMigration()

        #expect(!migrator.requiresMigration())
        let room = try readFirst("CDChatRoomMetadata", from: storeURL)
        #expect(room["name"] as? String == "Lobby")
        #expect(room["chatRelativeId"] as? String == "lobby")
        #expect(room["configGeneration"] as? Int64 == 0)
        #expect(room["badgeKind"] as? Int16 == 0)
        #expect(room["config"] is NSNull)
        #expect(room["previewMessage"] is NSNull)

        let message = try readFirst("CDChatMessage", from: storeURL)
        #expect(message["messageId"] as? String == "m1")
    }

    /// The facade opens the current model, so the app reads the declarations of a room through their mappers.
    @Test func theCurrentModelKeepsTheDeclarationsOfARoom() async throws {
        #expect(UserStorageParams.modelVersion == .version56)
        #expect(UserStorageVersion.version55.nextVersion() == .version56)
        #expect(UserStorageVersion.version56.nextVersion() == nil)

        let facade = UserDataStorageTestFacade()
        let config = Chat.RoomConfig(productSource: true, reactionDetails: false, reportSeen: true)
        let preview = Chat.RoomPreview(timestamp: 42, badge: .unreadCount(3), lastMessage: nil, unseenReaction: true)
        let chat = Chat.LocalModel.newChatWithRoom(
            extensionId: "product.dot",
            roomId: "lobby",
            roomMetadata: Chat.RoomMetadata(chatRelativeId: "lobby", name: "Lobby", icon: nil)
        )
        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .saveOperation({ [chat] }, { [] })
            .asyncExecute()
        try await save(Chat.RoomConfigUpdate(chatId: roomChatId, config: config), in: facade)
        try await save(Chat.RoomPreviewUpdate(chatId: roomChatId, preview: preview), in: facade)

        let stored = try await facade.createRepository(mapper: AnyCoreDataMapper(ChatRoomDeclarationsMapper()))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .first { $0.chatId == self.roomChatId.rawRepresentation }?
            .declarations

        #expect(stored == Chat.RoomDeclarations(config: config, configGeneration: 1, preview: preview))
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
            model: UserStorageParams.modelVersion,
            fileManager: .default
        )
    }

    func save<Update: ChatRoomDeclarationUpdating>(_ update: Update, in facade: StorageFacadeProtocol) async throws {
        try await facade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatRoomDeclarationUpdateMapper<Update>())
        )
        .saveOperation({ [update] }, { [] })
        .asyncExecute()
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

    /// A room with a message of model 55.
    func writeV55Data(to storeURL: URL) throws {
        try write(.version55, to: storeURL) { context in
            let chat = NSEntityDescription.insertNewObject(forEntityName: "CDChat", into: context)
            chat.setValue(self.roomChatId.rawRepresentation, forKey: "identifier")
            chat.setValue(Int16(1), forKey: "chatType")
            chat.setValue("product.dot", forKey: "chatTypeContext")

            let metadata = NSEntityDescription.insertNewObject(forEntityName: "CDChatRoomMetadata", into: context)
            metadata.setValue("lobby", forKey: "chatRelativeId")
            metadata.setValue("Lobby", forKey: "name")
            chat.setValue(metadata, forKey: "roomMetadata")

            let message = NSEntityDescription.insertNewObject(forEntityName: "CDChatMessage", into: context)
            message.setValue("m1", forKey: "messageId")
            message.setValue(chat, forKey: "chat")
        }
    }

    /// Reads the attributes inside the context: a managed object has no values after its context is gone.
    func readFirst(_ entityName: String, from storeURL: URL) throws -> [String: Any] {
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
