import CoreData
import Foundation
import Testing

@testable import polkadot_app

struct ChatCoinageMigrationTests {
    @Test("main and Chat stores retain payments, durable records and pocket cards", arguments: [
        UserStorageVersion.version49,
        .version49Chat,
        .version52,
        .version53,
        .version53Chat
    ])
    func preservesPaymentOwnershipAndLedger(from version: UserStorageVersion) throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let storeURL = directory.appendingPathComponent("payments.sqlite")
        let migrator = UserStorageMigrator(
            storeURL: storeURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: UserStorageParams.modelVersion,
            fileManager: .default
        )
        let source = migrator.createManagedObjectModel(
            forResource: version.rawValue,
            modelDirectory: UserStorageParams.modelDirectory
        )
        let owner = Data(repeating: 0x42, count: 32)
        let payload = Data([0x01, 0x02, 0x03])
        let isChatStore = version == .version49Chat || version == .version53Chat
        let hasPocketCards = version == .version53
        try withStore(model: source, url: storeURL) { context in
            let payment = NSEntityDescription.insertNewObject(forEntityName: "CDIncomingPayment", into: context)
            payment.setValue("group-1", forKey: "identifier")
            payment.setValue("payment-1", forKey: "paymentId")
            payment.setValue("chat.paseo", forKey: "productId")
            payment.setValue("12345", forKey: "amount")
            payment.setValue(Date(timeIntervalSince1970: 100), forKey: "createdAt")
            if isChatStore {
                payment.setValue(owner, forKey: "ownerId")
                let record = NSEntityDescription.insertNewObject(forEntityName: "CDNativeCoinageRecord", into: context)
                record.setValue("ledger-1", forKey: "identifier")
                record.setValue(payload, forKey: "payload")
            }
            if hasPocketCards {
                let card = NSEntityDescription.insertNewObject(forEntityName: "CDPocketCard", into: context)
                card.setValue("card-1", forKey: "identifier")
                card.setValue("card-1", forKey: "cardId")
                card.setValue("cards.paseo", forKey: "productId")
                card.setValue("Saved card", forKey: "title")
                card.setValue(payload, forKey: "face")
                card.setValue(Date(timeIntervalSince1970: 100), forKey: "addedAt")
            }
            try context.save()
        }

        #expect(migrator.requiresMigration())
        migrator.performMigration()
        #expect(migrator.requiresMigration() == false)

        let destination = migrator.createManagedObjectModel(
            forResource: UserStorageParams.modelVersion.rawValue,
            modelDirectory: UserStorageParams.modelDirectory
        )
        try withStore(model: destination, url: storeURL) { context in
            let payments = try context.fetch(NSFetchRequest<NSManagedObject>(entityName: "CDIncomingPayment"))
            let payment = try #require(payments.first)
            #expect(payments.count == 1)
            #expect(payment.value(forKey: "identifier") as? String == "group-1")
            #expect(payment.value(forKey: "amount") as? String == "12345")
            #expect(payment.value(forKey: "ownerId") as? Data == (isChatStore ? owner : nil))
            let records = try context.fetch(NSFetchRequest<NSManagedObject>(entityName: "CDNativeCoinageRecord"))
            if isChatStore {
                #expect(records.count == 1)
                let record = try #require(records.first)
                #expect(record.value(forKey: "identifier") as? String == "ledger-1")
                #expect(record.value(forKey: "payload") as? Data == payload)
            } else {
                #expect(records.isEmpty)
            }
            let cards = try context.fetch(NSFetchRequest<NSManagedObject>(entityName: "CDPocketCard"))
            if hasPocketCards {
                #expect(cards.count == 1)
                let card = try #require(cards.first)
                #expect(card.value(forKey: "identifier") as? String == "card-1")
                #expect(card.value(forKey: "cardId") as? String == "card-1")
                #expect(card.value(forKey: "productId") as? String == "cards.paseo")
                #expect(card.value(forKey: "title") as? String == "Saved card")
                #expect(card.value(forKey: "face") as? Data == payload)
                #expect(card.value(forKey: "addedAt") as? Date == Date(timeIntervalSince1970: 100))
            } else {
                #expect(cards.isEmpty)
            }
        }
    }

    private func withStore(
        model: NSManagedObjectModel,
        url: URL,
        body: (NSManagedObjectContext) throws -> Void
    ) throws {
        let coordinator = NSPersistentStoreCoordinator(managedObjectModel: model)
        let store = try coordinator.addPersistentStore(
            ofType: NSSQLiteStoreType,
            configurationName: nil,
            at: url,
            options: [NSPersistentHistoryTrackingKey: true]
        )
        defer { try? coordinator.remove(store) }
        let context = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        context.persistentStoreCoordinator = coordinator
        try context.performAndWait { try body(context) }
    }
}
