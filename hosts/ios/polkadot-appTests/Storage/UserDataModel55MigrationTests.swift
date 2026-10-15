import CoreData
import Foundation
import Keystore_iOS
import Operation_iOS
import Testing
@testable import polkadot_app

/// `UserDataModel55` adds the room declarations of Chat Extension v2 (02-DESIGN §8.2).
final class UserDataModel55MigrationTests {
    private let directory = URL.temporaryDirectory.appending(path: "model55-\(UUID().uuidString)")
    private let roomChatId = Chat.Id.chatExtension("product.dot", roomId: "lobby")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func aStoreWithV1ProductRoomsMigratesWithItsRoomsAndNoDeclarations() throws {
        let storeURL = directory.appending(path: UserStorageParams.databaseName)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try writeV54Room(to: storeURL)

        let migrator = UserStorageMigrator(
            storeURL: storeURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: .version55,
            fileManager: .default
        )
        #expect(migrator.requiresMigration())

        migrator.performMigration()

        #expect(!migrator.requiresMigration())
        let room = try readRoomMetadata(from: storeURL)
        #expect(room["name"] as? String == "Lobby")
        #expect(room["icon"] as? String == "https://example.com/lobby.png")
        #expect(room["roomDescription"] is NSNull)
        #expect(room["configGeneration"] as? Int64 == 0)
        #expect(room["avatarKind"] as? Int16 == 0)
        #expect(room["previewOngoing"] as? Bool == false)
    }

    @Test func eachDeclarationUpdateWritesOnlyItsOwnAttributes() async throws {
        let facade = UserDataStorageTestFacade()
        let context = ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: StubLogger(),
            messageOrderAllocator: InMemoryChatMessageOrderAllocator()
        )
        let bot = ChatSampleExtension(logger: StubLogger())
        let chatId = Chat.Id.chatExtension(bot.identifier, roomId: "lobby")
        _ = try await context.createRoom(for: bot, roomId: "lobby", name: "Lobby", icon: nil)
        try await context.setRoomFooter(for: bot, roomId: "lobby", footer: .empty)

        let preview = Chat.RoomPreview(
            order: .pinToTop,
            timestamp: 42,
            badge: .unreadCount(3),
            text: "Your turn",
            ongoing: true
        )
        try await save(Chat.RoomDescriptionUpdate(chatId: chatId, description: "Weekly game", avatar: .none), facade)
        let config = Chat.RoomConfig(
            productSource: false,
            reactionDetails: true,
            participants: false,
            messageActions: false,
            participantActions: false,
            allowedReactions: ["👍"],
            placeholder: "Say hi",
            reportSeen: true
        )
        let rules = Chat.RoomToolbarAction(
            actionId: "rules",
            label: "Rules",
            icon: .preimage(key: Data(repeating: 7, count: 32)),
            handler: .callback
        )
        let toolbar = [rules]
        try await save(Chat.RoomConfigUpdate(chatId: chatId, config: .hostDefault), facade)
        try await save(Chat.RoomConfigUpdate(chatId: chatId, config: config), facade)
        try await save(Chat.RoomPreviewUpdate(chatId: chatId, preview: preview), facade)
        try await save(Chat.RoomToolbarUpdate(chatId: chatId, actions: toolbar), facade)

        let stored = try await facade.makeRepo(mapper: ChatRoomDeclarationsMapper())
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        let declarations = try #require(stored.first { $0.chatId == chatId.rawRepresentation }).declarations
        #expect(declarations == Chat.RoomDeclarations(
            description: "Weekly game",
            avatar: .none,
            config: config,
            configGeneration: 2,
            preview: preview,
            toolbar: toolbar,
            suggestions: nil,
            commands: nil,
            footerConfig: nil
        ))

        let room = try await facade.makeRepo(mapper: ChatModelMapper())
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
        #expect(room?.roomMetadata?.name == "Lobby")
        #expect(room?.roomFooter == .empty)
    }
}

private extension UserDataModel55MigrationTests {
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

    func writeV54Room(to storeURL: URL) throws {
        let coordinator = try openStore(.version54, storeURL: storeURL)
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator

        try context.performAndWait {
            let chat = NSEntityDescription.insertNewObject(forEntityName: "CDChat", into: context)
            chat.setValue(roomChatId.rawRepresentation, forKey: "identifier")
            chat.setValue(Int16(1), forKey: "chatType")
            chat.setValue("product.dot", forKey: "chatTypeContext")

            let metadata = NSEntityDescription.insertNewObject(forEntityName: "CDChatRoomMetadata", into: context)
            metadata.setValue("lobby", forKey: "chatRelativeId")
            metadata.setValue("Lobby", forKey: "name")
            metadata.setValue("https://example.com/lobby.png", forKey: "icon")
            chat.setValue(metadata, forKey: "roomMetadata")

            try context.save()
        }

        for store in coordinator.persistentStores {
            try coordinator.remove(store)
        }
    }

    /// Reads the attributes inside the context: a managed object has no values once its context is gone. The
    /// migrator moves a store to the current model, so the read opens that model.
    func readRoomMetadata(from storeURL: URL) throws -> [String: Any] {
        let coordinator = try openStore(UserStorageParams.modelVersion, storeURL: storeURL)
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator

        return try context.performAndWait {
            let request = NSFetchRequest<NSManagedObject>(entityName: "CDChatRoomMetadata")
            let room = try #require(try context.fetch(request).first)
            let keys = Array(room.entity.attributesByName.keys)
            return room.dictionaryWithValues(forKeys: keys)
        }
    }

    func save<Update: ChatRoomDeclarationUpdating>(
        _ update: Update,
        _ facade: UserDataStorageTestFacade
    ) async throws {
        let repository = facade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatRoomDeclarationUpdateMapper<Update>())
        )
        try await repository.saveOperation({ [update] }, { [] }).asyncExecute()
    }
}
