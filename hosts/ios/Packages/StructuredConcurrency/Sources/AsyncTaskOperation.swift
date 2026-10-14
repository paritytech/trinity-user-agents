import Foundation
import os
import Operation_iOS

open class AsyncTaskOperation<ResultType>: BaseOperation<ResultType>, @unchecked Sendable {
    private let closure: () async throws -> ResultType
    private let taskLock = OSAllocatedUnfairLock<Task<Void, Never>?>(initialState: nil)

    public init(_ closure: @escaping () async throws -> ResultType) {
        self.closure = closure
    }

    override open func performAsync(_ callback: @escaping (Result<ResultType, Error>) -> Void) throws {
        let task = Task {
            do {
                let value = try await closure()
                callback(.success(value))
            } catch {
                callback(.failure(error))
            }
        }

        taskLock.withLock { $0 = task }

        if isCancelled {
            task.cancel()
        }
    }

    override open func cancel() {
        taskLock.withLock { $0?.cancel() }
        super.cancel()
    }
}
