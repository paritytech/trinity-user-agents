@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockDeviceSyncUpdateIdProvider: DeviceSyncUpdateIdProviding {
    private let lock = NSLock()
    private var next: UInt32

    init(startingAt next: UInt32 = 1) {
        self.next = next
    }

    func nextId() -> UInt32 {
        lock.withLock {
            let id = next
            next += 1
            return id
        }
    }
}
