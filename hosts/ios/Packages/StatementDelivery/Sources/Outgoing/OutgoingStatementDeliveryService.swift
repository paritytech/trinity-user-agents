import BackgroundExecution
import Foundation
import StatementStore

public protocol OutgoingStatementDelivering<Input>: Sendable {
    associatedtype Input: OutgoingStatementInput

    /// First delivery: retries with backoff for as long as the statement awaits it.
    func deliverUntilPublished(_ input: Input) async

    /// One attempt with a signer resolved by the caller; joins an attempt already in flight.
    func deliverOnce(_ input: Input, signer: StatementStoreSigning, period: UInt32) async throws
}

public struct OutgoingStatementDeliveryDependencies<Input: OutgoingStatementInput>: Sendable {
    let store: any OutgoingStatementStoring<Input>
    let resolver: any StatementSignerResolving<Input>
    let service: any OutgoingStatementServicing<Input>
    let builder: any OutgoingStatementBuilding<Input>
    let postDelivery: (any OutgoingStatementPostDelivering<Input>)?

    public init(
        store: any OutgoingStatementStoring<Input>,
        resolver: any StatementSignerResolving<Input>,
        service: any OutgoingStatementServicing<Input>,
        builder: any OutgoingStatementBuilding<Input>,
        postDelivery: (any OutgoingStatementPostDelivering<Input>)?
    ) {
        self.store = store
        self.resolver = resolver
        self.service = service
        self.builder = builder
        self.postDelivery = postDelivery
    }
}

public struct OutgoingStatementDeliveryExecution: Sendable {
    let backgroundExecutor: BackgroundExecuting
    let clock: any Clock<Duration>

    public init(backgroundExecutor: BackgroundExecuting, clock: any Clock<Duration> = ContinuousClock()) {
        self.backgroundExecutor = backgroundExecutor
        self.clock = clock
    }
}

public actor OutgoingStatementDeliveryService<Input: OutgoingStatementInput> {
    private static var firstRetryDelay: Duration { .seconds(2) }
    private static var maxRetryDelay: Duration { .seconds(5 * 60) }
    private static var maxBackoffDoublings: Int { 8 }

    private let dependencies: OutgoingStatementDeliveryDependencies<Input>
    private let execution: OutgoingStatementDeliveryExecution
    private let diagnostics: OutgoingStatementDiagnostics
    private var attempts: [String: Task<Void, Error>] = [:]
    private(set) var joinedAttemptCount = 0

    public init(
        dependencies: OutgoingStatementDeliveryDependencies<Input>,
        execution: OutgoingStatementDeliveryExecution,
        diagnostics: OutgoingStatementDiagnostics
    ) {
        self.dependencies = dependencies
        self.execution = execution
        self.diagnostics = diagnostics
    }
}

extension OutgoingStatementDeliveryService: OutgoingStatementDelivering {
    public func deliverUntilPublished(_ input: Input) async {
        var attempt = 0

        while await isAwaitingFirstDelivery(input) {
            do {
                try await execution.backgroundExecutor.execute { try await self.deliverFirst(input) }
                diagnostics.issues.deliveryStalled.recordRecovery(for: input.statementId)
                return
            } catch let error as OutgoingStatementTerminalError {
                await markFailed(input, failure: .terminal(error))
                return
            } catch {
                attempt += 1
                diagnostics.logger.error("Statement \(input.statementId) attempt \(attempt) failed: \(error)")
                diagnostics.issues.deliveryStalled.recordFailure(for: input.statementId, error: error, counters: [:])

                do {
                    try await execution.clock.sleep(for: Self.retryDelay(forAttempt: attempt))
                } catch {
                    return
                }
            }
        }
    }

    public func deliverOnce(_ input: Input, signer: StatementStoreSigning, period: UInt32) async throws {
        try await attempt(for: input) { [dependencies] in
            try await dependencies.service.publish(input, signer: signer)
            let publication = try await dependencies.store.markPublished(input, period: period)

            if publication.isFirst {
                await dependencies.postDelivery?.didPublishFirstTime(input)
            }
        }
    }
}

private extension OutgoingStatementDeliveryService {
    func deliverFirst(_ input: Input) async throws {
        let size = try dependencies.builder.encodedSize(of: input)

        guard try await size <= dependencies.resolver.maxStatementSize() else {
            diagnostics.logger.error("Statement \(input.statementId) is \(size) bytes; marking it failed")
            diagnostics.issues.oversized.recordFailure(for: input.statementId, error: nil, counters: ["size": size])
            await markFailed(input, failure: .oversized(size: size))
            return
        }

        let period = try await dependencies.resolver.currentPeriod()
        let resolved = try await dependencies.resolver.resolve([input], period: period)[input.statementId]

        guard let resolved else {
            throw OutgoingStatementDeliveryError.signerNotResolved
        }

        try await deliverOnce(input, signer: resolved.get(), period: period)
    }

    // Joins a running attempt for the same statement instead of publishing twice.
    func attempt(for input: Input, operation: @escaping @Sendable () async throws -> Void) async throws {
        if let running = attempts[input.statementId] {
            joinedAttemptCount += 1
            return try await running.value
        }

        let task = Task { try await operation() }
        attempts[input.statementId] = task
        defer { attempts[input.statementId] = nil }

        try await task.value
    }

    func isAwaitingFirstDelivery(_ input: Input) async -> Bool {
        guard !Task.isCancelled else { return false }

        do {
            return try await dependencies.store.isAwaitingFirstDelivery(input)
        } catch {
            diagnostics.logger.error("Statement \(input.statementId) state unreadable: \(error)")
            diagnostics.issues.deliveryStateUnreadable.recordFailure(
                for: input.statementId,
                error: error,
                counters: [:]
            )
            return false
        }
    }

    func markFailed(_ input: Input, failure: OutgoingStatementFailure) async {
        do {
            try await dependencies.store.markFailed(input, failure: failure)
        } catch {
            diagnostics.logger.error("Statement \(input.statementId) not marked failed: \(error)")
        }
    }

    static func retryDelay(forAttempt attempt: Int) -> Duration {
        let doublings = min(attempt - 1, maxBackoffDoublings)
        return min(firstRetryDelay * (1 << doublings), maxRetryDelay)
    }
}

public enum OutgoingStatementDeliveryError: Error {
    case signerNotResolved
}
