import Foundation
import Operation_iOS
import os

public extension BaseOperation {
    func asyncExecute() async throws -> ResultType {
        try await CompoundOperationWrapper(targetOperation: self).asyncExecute()
    }
}

public extension CompoundOperationWrapper {
    func asyncExecute() async throws -> ResultType {
        let coordinator = AsyncExecuteCoordinator<ResultType>()

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                coordinator.start(with: continuation, wrapper: self)
            }
        } onCancel: {
            coordinator.cancel(wrapper: self)
        }
    }
}

private final class AsyncExecuteCoordinator<ResultType>: @unchecked Sendable {
    private enum Outcome {
        case submitted
        case cancelled
        case finished
    }

    private let lock = OSAllocatedUnfairLock(initialState: false)

    func start(
        with continuation: CheckedContinuation<ResultType, Error>,
        wrapper: CompoundOperationWrapper<ResultType>
    ) {
        let guarded = CheckedContinuationGuard(continuation)

        let outcome: Outcome = lock.withLock { didCancel in
            if didCancel || Task.isCancelled {
                didCancel = true
                wrapper.cancel()
                return .cancelled
            }

            if wrapper.targetOperation.isFinished {
                return .finished
            }

            let target = wrapper.targetOperation
            target.completionBlock = {
                guarded.resume(Result { try target.extractNoCancellableResultData() })
            }

            OperationManagerFacade.sharedDefaultQueue.addOperations(
                wrapper.allOperations,
                waitUntilFinished: false
            )
            return .submitted
        }

        switch outcome {
        case .submitted:
            break
        case .cancelled:
            guarded.resume(throwing: CancellationError())
        case .finished:
            guarded.resume(Result { try wrapper.targetOperation.extractNoCancellableResultData() })
        }
    }

    func cancel(wrapper: CompoundOperationWrapper<ResultType>) {
        lock.withLock { didCancel in
            didCancel = true
            wrapper.cancel()
        }
    }
}
