import os
import SDKLogger
import UIKit

public struct BackgroundExecutor {
    private let host: any BackgroundTaskHost
    private let logger: SDKLoggerProtocol?

    public init(
        host: any BackgroundTaskHost = UIApplication.shared,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.host = host
        self.logger = logger
    }
}

// MARK: - Private

private extension BackgroundExecutor {
    func end(_ tokenLock: OSAllocatedUnfairLock<BackgroundTaskToken?>) async {
        guard let token = claimToken(from: tokenLock) else { return }

        await host.endBackgroundTask(token)
        logger?.debug("BackgroundExecution: ended (token \(token.rawValue))")
    }

    func claimToken(from lock: OSAllocatedUnfairLock<BackgroundTaskToken?>) -> BackgroundTaskToken? {
        lock.withLock { token in
            let claimed = token
            token = nil
            return claimed
        }
    }
}

// MARK: - BackgroundExecuting

extension BackgroundExecutor: @unchecked Sendable, BackgroundExecuting {
    public func execute<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        let workTask = Task { try await operation() }
        let tokenLock = OSAllocatedUnfairLock<BackgroundTaskToken?>(initialState: nil)

        let token = await host.beginBackgroundTask { [host, tokenLock, workTask] in
            let tokenToEnd = claimToken(from: tokenLock)
            workTask.cancel()

            guard let tokenToEnd else { return }

            /// Per Apple's doc:
            /// Use this handler to clean up and mark the end of the background task. Failure to end the task explicitly
            /// will result in the termination of the app. The system calls the handler synchronously on the main
            /// thread, blocking the app’s suspension momentarily.
            /// https://developer.apple.com/documentation/uikit/uiapplication/beginbackgroundtask(withname:expirationhandler:)#parameters
            MainActor.assumeIsolated { host.endBackgroundTask(tokenToEnd) }
        }

        tokenLock.withLock { $0 = token.isValid ? token : nil }

        logger?.debug(
            token.isValid
                ? "BackgroundExecution: started (token \(token.rawValue))"
                : "BackgroundExecution: no background time granted, running inline"
        )

        do {
            let result = try await withTaskCancellationHandler {
                try await workTask.value
            } onCancel: {
                workTask.cancel()
            }

            await end(tokenLock)
            return result
        } catch {
            await end(tokenLock)

            if !Task.isCancelled, error is CancellationError {
                throw BackgroundExecutionExpiredError()
            }

            throw error
        }
    }
}
