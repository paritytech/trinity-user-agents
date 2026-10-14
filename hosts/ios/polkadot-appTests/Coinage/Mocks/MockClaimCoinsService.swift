import AsyncExtensions
import Coinage
import Foundation

/// Replays a scripted detection sequence for every claim and records what was asked.
final class MockClaimCoinsService: ClaimCoinsServicing, @unchecked Sendable {
    struct Call: Equatable {
        let coinKeys: [Data]
        let groupId: CoinageTxGroupId
        let retryUntil: Date
    }

    private let lock = NSLock()
    private var recordedCalls: [Call] = []

    let detections: [CoinageTransferDetection]
    let failure: Error?

    init(detections: [CoinageTransferDetection], failure: Error? = nil) {
        self.detections = detections
        self.failure = failure
    }

    var calls: [Call] {
        lock.lock()
        defer { lock.unlock() }
        return recordedCalls
    }

    func claim(
        coinKeys: [Data],
        groupId: CoinageTxGroupId,
        retryUntil: Date,
        context _: DenominationBreakdownContext
    ) -> AnyAsyncSequence<CoinageTransferDetection> {
        lock.lock()
        recordedCalls.append(Call(coinKeys: coinKeys, groupId: groupId, retryUntil: retryUntil))
        lock.unlock()

        let detections = detections
        let failure = failure

        return AsyncThrowingStream<CoinageTransferDetection, Error> { continuation in
            for detection in detections {
                continuation.yield(detection)
            }
            if let failure {
                continuation.finish(throwing: failure)
            } else {
                continuation.finish()
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
