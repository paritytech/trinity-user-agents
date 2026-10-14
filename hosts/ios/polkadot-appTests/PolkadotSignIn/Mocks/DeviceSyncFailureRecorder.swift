@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

actor DeviceSyncFailureRecorder {
    private(set) var recordedFailure: (accountId: Data, failure: DeviceSyncConnectionFailure)?
    private var waiter: CheckedContinuation<(accountId: Data, failure: DeviceSyncConnectionFailure), Never>?

    func record(accountId: Data, failure: DeviceSyncConnectionFailure) {
        recordedFailure = (accountId, failure)
        waiter?.resume(returning: (accountId, failure))
        waiter = nil
    }

    func waitForRecordedFailure() async -> (accountId: Data, failure: DeviceSyncConnectionFailure) {
        if let recordedFailure { return recordedFailure }
        return await withCheckedContinuation { waiter = $0 }
    }
}
