import AsyncExtensions
import Coinage
import Foundation
import SubstrateSdk

/// Replays a scripted sequence of per-coin statuses for every subscription and records what was asked.
final class MockCoinageTransferStatusService: CoinageTransferStatusServicing, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedCoinKeys: [[Data]] = []
    private var recordedGroupIds: [CoinageTxGroupId] = []

    let snapshots: [[PublicKey: CoinageTransferState]]

    init(snapshots: [[PublicKey: CoinageTransferState]]) {
        self.snapshots = snapshots
    }

    var subscriptions: [[Data]] {
        lock.lock()
        defer { lock.unlock() }
        return recordedCoinKeys
    }

    var reclaimGroupIds: [CoinageTxGroupId] {
        lock.lock()
        defer { lock.unlock() }
        return recordedGroupIds
    }

    func subscribeStatuses(
        coinKeys: [Data],
        reclaimGroupId: CoinageTxGroupId
    ) -> AnyAsyncSequence<[PublicKey: CoinageTransferState]> {
        lock.lock()
        recordedCoinKeys.append(coinKeys)
        recordedGroupIds.append(reclaimGroupId)
        lock.unlock()

        let snapshots = snapshots

        return AsyncThrowingStream<[PublicKey: CoinageTransferState], Error> { continuation in
            for snapshot in snapshots {
                continuation.yield(snapshot)
            }
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }
}
