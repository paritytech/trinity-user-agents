@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockLastSyncOfferIdRepositoryFactory: LastSyncOfferIdRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.LastSyncOfferIdUpdate>()
    private let lock = NSLock()
    private let savedUpdateEvents = DeviceSyncTestEventRecorder<Chat.LastSyncOfferIdUpdate>()
    private var _savedUpdates = [Chat.LastSyncOfferIdUpdate]()

    var savedUpdates: [Chat.LastSyncOfferIdUpdate] {
        lock.withLock { _savedUpdates }
    }

    func waitForSavedUpdate() async -> Chat.LastSyncOfferIdUpdate {
        let updates = await savedUpdateEvents.waitForCount(1)
        return updates[0]
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.LastSyncOfferIdUpdate> {
        AnyDataProviderRepository(MockLastSyncOfferIdRepository(
            repository: repository,
            onSave: { [weak self] updates in
                self?.lock.withLock { self?._savedUpdates.append(contentsOf: updates) }
                updates.forEach { self?.savedUpdateEvents.record($0) }
            }
        ))
    }
}
