@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockOutgoingUpdateTimeRepositoryFactory: OutgoingUpdateTimeRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.OutgoingUpdateTimeUpdate>()
    private let lock = NSLock()
    private let savedUpdateEvents = DeviceSyncTestEventRecorder<Chat.OutgoingUpdateTimeUpdate>()
    private var _savedUpdates = [Chat.OutgoingUpdateTimeUpdate]()

    var savedUpdates: [Chat.OutgoingUpdateTimeUpdate] {
        lock.withLock { _savedUpdates }
    }

    func waitForSavedUpdate() async -> Chat.OutgoingUpdateTimeUpdate {
        let updates = await savedUpdateEvents.waitForCount(1)
        return updates[0]
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.OutgoingUpdateTimeUpdate> {
        AnyDataProviderRepository(MockOutgoingUpdateTimeRepository(
            repository: repository,
            onSave: { [weak self] updates in
                self?.lock.withLock { self?._savedUpdates.append(contentsOf: updates) }
                updates.forEach { self?.savedUpdateEvents.record($0) }
            }
        ))
    }
}
