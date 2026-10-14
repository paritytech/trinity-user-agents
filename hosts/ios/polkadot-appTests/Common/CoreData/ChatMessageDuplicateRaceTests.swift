import CoreData
import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

/// Reproduces the duplicate-message bug that occurs when the main app and the
/// `NotificationServiceExtension` both insert a `CDChatMessage` with the same
/// `messageId` into the shared store from their own processes.
///
/// Each process runs an independent `NSPersistentStoreCoordinator` that points
/// at the same SQLite file. The fix is a Core Data uniqueness constraint on
/// `CDChatMessage.messageId` combined with `NSMergeByPropertyObjectTrumpMergePolicy`,
/// which Operation-iOS's `CoreDataService.setup()` installs whenever history
/// tracking is enabled. With those pieces in place, the second coordinator's
/// save is merged into the first row instead of creating a new one, and each
/// process's context then picks up the other's changes through the persistent
/// history pipeline (`CoreDataHistoryObserver`/`Fetcher`/`Merger`).
///
/// The test therefore uses two separate `CoreDataService` instances pointing at
/// the same SQLite file — one per transaction author — so that the conflict is
/// actually resolved through the cross-process path rather than through
/// sibling contexts sharing a single coordinator.
@Suite("CDChatMessage uniqueness constraint (cross-process)")
struct ChatMessageDuplicateRaceTests {
    private let sharedDatabaseDirectory: URL
    private let sharedContainerName: String
    private let appAuthor: String
    private let extensionAuthor: String

    init() {
        let suiteName = "ChatMessageDuplicateRaceTests.\(UUID().uuidString)"
        sharedDatabaseDirectory = FileManager.default
            .temporaryDirectory
            .appendingPathComponent(suiteName, isDirectory: true)
        sharedContainerName = suiteName
        appAuthor = "test.app.\(UUID().uuidString)"
        extensionAuthor = "test.extension.\(UUID().uuidString)"
    }

    @Test("two coordinators saving the same messageId resolve to a single row")
    func twoCoordinatorsSavingSameMessageIdProduceOneRow() async throws {
        let messageId = "duplicate-message-id"

        let appService = makeService(author: appAuthor)
        let extensionService = makeService(author: extensionAuthor)

        defer {
            try? appService.close()
            try? extensionService.close()
            cleanupSharedResources()
        }

        // Extension writes first — simulates the notification extension receiving
        // a push while the app is suspended.
        try await insertMessage(messageId: messageId, timestamp: 100, using: extensionService)

        // App writes the same messageId — simulates the user returning to the
        // foreground and the app committing its own copy before it has merged
        // remote history.
        try await insertMessage(messageId: messageId, timestamp: 200, using: appService)

        let appCount = try await countMessages(messageId: messageId, in: appService)
        let extensionCount = try await countMessages(messageId: messageId, in: extensionService)

        #expect(
            appCount == 1,
            "App context expected uniqueness constraint to merge duplicate, got \(appCount) rows."
        )
        #expect(
            extensionCount == 1,
            "Extension context expected uniqueness constraint to merge duplicate, got \(extensionCount) rows."
        )
    }
}

private extension ChatMessageDuplicateRaceTests {
    func makeService(author: String) -> CoreDataServiceProtocol {
        let modelName = UserStorageParams.modelVersion.rawValue
        let subdirectory = UserStorageParams.modelDirectory
        let bundle = Bundle.main

        let omoURL = bundle.url(
            forResource: modelName,
            withExtension: "omo",
            subdirectory: subdirectory
        )
        let momURL = bundle.url(
            forResource: modelName,
            withExtension: "mom",
            subdirectory: subdirectory
        )
        let modelURL = omoURL ?? momURL

        let tracking = CoreDataHistoryTrackingSettings(
            transactionAuthor: author,
            targets: [appAuthor, extensionAuthor],
            sharedContainerName: sharedContainerName
        )

        let persistentSettings = CoreDataPersistentSettings(
            databaseDirectory: sharedDatabaseDirectory,
            databaseName: "UserDataModel.sqlite",
            incompatibleModelStrategy: .removeStore,
            excludeFromiCloudBackup: true,
            historyTracking: tracking
        )

        let configuration = CoreDataServiceConfiguration(
            modelURL: modelURL!,
            storageType: .persistent(settings: persistentSettings)
        )

        return CoreDataService(configuration: configuration)
    }

    func insertMessage(
        messageId: String,
        timestamp: Int64,
        using service: CoreDataServiceProtocol
    ) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            service.performAsync { context, error in
                guard let context else {
                    continuation.resume(throwing: error ?? CoreDataRepositoryError.undefined)
                    return
                }
                do {
                    let entityName = String(describing: CDChatMessage.self)
                    let entity = NSEntityDescription.insertNewObject(
                        forEntityName: entityName,
                        into: context
                    ) as! CDChatMessage

                    entity.messageId = messageId
                    entity.status = Chat.LocalMessage.Status.incoming(.new).rawValue
                    entity.timestamp = timestamp
                    entity.contentType = 0
                    entity.originType = 0

                    try context.save()
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func countMessages(
        messageId: String,
        in service: CoreDataServiceProtocol
    ) async throws -> Int {
        try await withCheckedThrowingContinuation { continuation in
            service.performAsync { context, error in
                guard let context else {
                    continuation.resume(throwing: error ?? CoreDataRepositoryError.undefined)
                    return
                }
                let request = NSFetchRequest<NSManagedObject>(
                    entityName: String(describing: CDChatMessage.self)
                )
                request.predicate = NSPredicate(
                    format: "%K == %@",
                    #keyPath(CDChatMessage.messageId),
                    messageId
                )
                do {
                    let count = try context.count(for: request)
                    continuation.resume(returning: count)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func cleanupSharedResources() {
        try? FileManager.default.removeItem(at: sharedDatabaseDirectory)
        UserDefaults().removePersistentDomain(forName: sharedContainerName)
    }
}
