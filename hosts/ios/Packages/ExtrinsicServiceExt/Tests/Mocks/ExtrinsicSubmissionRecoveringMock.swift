import Foundation
@testable import ExtrinsicService
@testable import ExtrinsicServiceExt

final class ExtrinsicSubmissionRecoveringMock: ExtrinsicSubmissionRecovering {
    let recovered = TestEvent()

    private let mutex = NSLock()
    private var storedDecision: ExtrinsicSubmissionFailureRecovery = .abort
    private var storedOnRecover: (() -> Void)?
    private var storedFailures: [ExtrinsicSubmissionFailure] = []

    var decision: ExtrinsicSubmissionFailureRecovery {
        get { mutex.withLock { storedDecision } }
        set { mutex.withLock { storedDecision = newValue } }
    }

    var onRecover: (() -> Void)? {
        get { mutex.withLock { storedOnRecover } }
        set { mutex.withLock { storedOnRecover = newValue } }
    }

    var failures: [ExtrinsicSubmissionFailure] {
        mutex.withLock { storedFailures }
    }

    func recover(
        builtExtrinsic _: ExtrinsicBuiltModel,
        failure: ExtrinsicSubmissionFailure
    ) async -> ExtrinsicSubmissionFailureRecovery {
        let (onRecover, decision) = record(failure)

        recovered.signal()
        onRecover?()

        return decision
    }
}

private extension ExtrinsicSubmissionRecoveringMock {
    func record(
        _ failure: ExtrinsicSubmissionFailure
    ) -> ((() -> Void)?, ExtrinsicSubmissionFailureRecovery) {
        mutex.withLock {
            storedFailures.append(failure)
            return (storedOnRecover, storedDecision)
        }
    }
}
