import Foundation
@testable import ExtrinsicService

final class ExtrinsicSubmittingMock: ExtrinsicSubmitting {
    static let createdExtrinsicHash = "0xdeadbeef"

    var subscriptionIds: [UInt16] = [42, 57]

    let subscribed = TestEvent()
    let cancelled = TestEvent()

    private let mutex = NSLock()
    private var notificationClosures: [ExtrinsicSubscriptionStatusClosure] = []
    private var storedSubmitted: [ExtrinsicBuiltModel] = []
    private var storedCancelled: [UInt16] = []
    private var storedSubmitCount = 0
    private var storedSubmitResults: [SubmitExtrinsicResult] = []
    private var storedAcceptedIds: [UInt16] = []
    private var storedRejectedIds: [UInt16] = []

    var submittedExtrinsics: [ExtrinsicBuiltModel] {
        mutex.withLock { storedSubmitted }
    }

    var submitAndSubscribeCount: Int { submittedExtrinsics.count }

    var cancelledIds: [UInt16] {
        mutex.withLock { storedCancelled }
    }

    var submitCount: Int {
        mutex.withLock { storedSubmitCount }
    }

    var rejectedIds: [UInt16] {
        mutex.withLock { storedRejectedIds }
    }

    func stubSubmitResults(_ results: [SubmitExtrinsicResult]) {
        mutex.withLock { storedSubmitResults = results }
    }

    func submit(
        builtExtrinsics _: [ExtrinsicBuiltModel],
        completion: @escaping ExtrinsicSubmitResultsClosure
    ) {
        let results: [SubmitExtrinsicResult] = mutex.withLock {
            storedSubmitCount += 1
            return storedSubmitResults
        }

        completion(results)
    }

    func submitAndSubscribe(
        builtExtrinsic: ExtrinsicBuiltModel,
        runningIn queue: DispatchQueue,
        subscriptionIdClosure: @escaping ExtrinsicSubscriptionIdClosure,
        notificationClosure: @escaping ExtrinsicSubscriptionStatusClosure
    ) {
        let index: Int = mutex.withLock {
            let index = storedSubmitted.count
            storedSubmitted.append(builtExtrinsic)
            notificationClosures.append(notificationClosure)
            return index
        }

        queue.async {
            notificationClosure(.success(Self.makeCreatedStatus(for: builtExtrinsic)))
        }

        let id = index < subscriptionIds.count ? subscriptionIds[index] : UInt16(100 + index)

        if subscriptionIdClosure(id) {
            mutex.withLock { storedAcceptedIds.append(id) }
        } else {
            mutex.withLock { storedRejectedIds.append(id) }
            cancelExtrinsicWatch(for: id)
        }

        subscribed.signal()
    }

    func cancelExtrinsicWatch(for identifier: UInt16) {
        mutex.withLock { storedCancelled.append(identifier) }
        cancelled.signal()
    }

    func emit(_ result: Result<ExtrinsicSubscribedStatusModel, Error>, toAttempt index: Int = 0) {
        let closure: ExtrinsicSubscriptionStatusClosure? = mutex.withLock {
            index < notificationClosures.count ? notificationClosures[index] : nil
        }

        closure?(result)
    }

    static func makeCreatedStatus(for builtExtrinsic: ExtrinsicBuiltModel) -> ExtrinsicSubscribedStatusModel {
        ExtrinsicSubscribedStatusModel(
            statusUpdate: ExtrinsicStatusUpdate(
                extrinsicHash: createdExtrinsicHash,
                extrinsicStatus: .created
            ),
            sender: builtExtrinsic.sender
        )
    }
}
