import AsyncExtensions
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing

@testable import polkadot_app

extension CoreDataBenchmarks {
    /// The two gates the concurrent stack must pass before it ships. Trivially true on `.serial`;
    /// phase 2 swaps the raw `perform` calls in B5 for `performWrite` / `performRead`.
    @Suite("CorrectnessGates", .serialized)
    struct CorrectnessGates {
        let scale = BenchmarkScale.default

        @Test("B5 read-your-writes", .timeLimit(.minutes(5)), arguments: StackVariant.supported)
        func readYourWrites(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)

            for index in 0 ..< scale.readYourWritesIterations {
                let identifier = "ryw-\(index)"

                try await facade.databaseService.performWrite { context in
                    let entityName = String(describing: CDChatContact.self)
                    let inserted = NSEntityDescription.insertNewObject(forEntityName: entityName, into: context)

                    guard let contact = inserted as? CDChatContact else {
                        throw BenchmarkError.entityInsertFailed(entityName)
                    }

                    contact.identifier = identifier
                    contact.username = identifier
                    contact.publicKey = BenchmarkFixtures.key(index)
                }

                let visible = try await facade.databaseService.performRead { context -> Int in
                    let request = NSFetchRequest<CDChatContact>(entityName: String(describing: CDChatContact.self))
                    request.predicate = NSPredicate(format: "%K == %@", #keyPath(CDChatContact.identifier), identifier)
                    return try context.count(for: request)
                }

                #expect(visible == 1, "write \(index) not visible to the next read")
            }
        }

        @Test("B6 observer sees a local save", .timeLimit(.minutes(1)), arguments: StackVariant.supported)
        func observerSeesLocalSave(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)
            let seeder = BenchmarkSeeder(facade: facade)
            let (tracker, consumer) = observeContacts(on: facade)
            defer { consumer.cancel() }

            await tracker.waitForDeliveries(subscription: 0, atLeast: 1)

            let contact = BenchmarkFixtures.contact(index: 1)
            try await seeder.contactRepository.saveOperation({ [contact] }, { [] }).asyncExecute()

            await tracker.waitForSize(subscription: 0, atLeast: 1)

            #expect(tracker.identifiers(subscription: 0) == [contact.identifier])
        }

        /// The Notification Service Extension path: a second coordinator with its own transaction author
        /// saves into the same file; the first facade must deliver it through persistent history.
        @Test("B6 observer sees a second-author save", .timeLimit(.minutes(1)), arguments: StackVariant.supported)
        func observerSeesSecondAuthorSave(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)
            let extensionFacade = try BenchmarkStorageFacade(
                variant: variant,
                author: BenchmarkStorageFacade.extensionAuthor,
                sharing: facade
            )
            let (tracker, consumer) = observeContacts(on: facade)
            defer { consumer.cancel() }

            await tracker.waitForDeliveries(subscription: 0, atLeast: 1)

            let contact = BenchmarkFixtures.contact(index: 2)
            let extensionSeeder = BenchmarkSeeder(facade: extensionFacade)
            try await extensionSeeder.contactRepository.saveOperation({ [contact] }, { [] }).asyncExecute()

            await tracker.waitForSize(subscription: 0, atLeast: 1)

            #expect(tracker.identifiers(subscription: 0) == [contact.identifier])
        }
    }
}

private extension CoreDataBenchmarks.CorrectnessGates {
    func observeContacts(on facade: BenchmarkStorageFacade) -> (DeliveryTracker, Task<Void, Never>) {
        let tracker = DeliveryTracker()
        let stream = facade.databaseService.subscribeSnapshot(mapper: AnyCoreDataMapper(ChatContactMapper()))

        let consumer = Task {
            do {
                for try await contacts in stream {
                    tracker.recordDelivery(
                        subscription: 0,
                        size: contacts.count,
                        identifiers: contacts.map(\.identifier)
                    )
                }
            } catch {
                print("[bench] contact subscription ended with \(error)")
            }
        }

        return (tracker, consumer)
    }
}
