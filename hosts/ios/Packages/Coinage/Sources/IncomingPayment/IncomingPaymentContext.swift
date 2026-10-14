import AsyncExtensions
import Foundation
import SDKLogger

/// Holds the in-flight incoming-payment claim tasks and each payment's current derived status,
/// keyed by `groupId`. A pure scheduler: it starts/dedups/queues runners and caches the live status
/// per payment. Persisting the verdict and wiping secrets is the service's job (it holds the record);
/// the context only tracks tasks and subjects.
actor IncomingPaymentContext {
    struct Pending {
        let groupId: CoinageTxGroupId
        let run: @Sendable () -> Task<Void, Never>
    }

    private let maxConcurrent: Int
    private let logger: SDKLoggerProtocol?

    private var lifeCycleTask: Task<Void, Never>?
    private var tasks: [CoinageTxGroupId: Task<Void, Never>] = [:]
    private var subjects: [CoinageTxGroupId: AsyncCurrentValueSubject<IncomingPaymentStatus>] = [:]
    private var pending: [Pending] = []

    init(maxConcurrent: Int = .max, logger: SDKLoggerProtocol?) {
        self.maxConcurrent = maxConcurrent
        self.logger = logger
    }
}

// MARK: - Scheduling

extension IncomingPaymentContext {
    func setup(_ setupClosure: () -> Task<Void, Never>?) {
        lifeCycleTask?.cancel()
        lifeCycleTask = nil
        lifeCycleTask = setupClosure()
    }

    func throttleIfNeeded() {
        lifeCycleTask?.cancel()
        lifeCycleTask = nil

        tasks.values.forEach { $0.cancel() }
        tasks.removeAll()
        pending.removeAll()
    }

    /// Starts (or queues, when at capacity) the claim for `groupId`. Dedups by `groupId` and seeds the
    /// status subject `.detecting`, so a subscriber attaching before the task reports sees detecting.
    func process(groupId: CoinageTxGroupId, run: @escaping @Sendable () -> Task<Void, Never>) {
        guard tasks[groupId] == nil, !pending.contains(where: { $0.groupId == groupId }) else {
            return
        }

        _ = subject(for: groupId)

        if tasks.count < maxConcurrent {
            tasks[groupId] = run()
        } else {
            pending.append(Pending(groupId: groupId, run: run))
        }
    }

    /// Emits a status to the payment's subject (the live channel subscribers read).
    func report(_ status: IncomingPaymentStatus, for groupId: CoinageTxGroupId) {
        subject(for: groupId).send(status)
    }

    /// The runner for `groupId` is done. Frees the slot and starts the next queued claim. The subject
    /// is kept so its last value stays queryable this session.
    func finish(groupId: CoinageTxGroupId) {
        tasks[groupId] = nil
        startNextIfPossible()
    }
}

// MARK: - Status

extension IncomingPaymentContext {
    func liveStatusStream(
        for groupId: CoinageTxGroupId,
        fallbackClosure: () async throws -> IncomingPaymentStatus
    ) async throws -> AnyAsyncSequence<IncomingPaymentStatus> {
        if let stream = subjects[groupId]?.eraseToAnyAsyncSequence() {
            return stream
        }

        let status = try await fallbackClosure()

        // we might race and receive status while being suspended
        if let subject = subjects[groupId]?.eraseToAnyAsyncSequence() {
            return subject
        }

        let subject = subject(for: groupId)
        subject.send(status)

        return subject.eraseToAnyAsyncSequence()
    }
}

// MARK: - Private

private extension IncomingPaymentContext {
    func subject(for groupId: CoinageTxGroupId) -> AsyncCurrentValueSubject<IncomingPaymentStatus> {
        if let subject = subjects[groupId] {
            return subject
        }
        let subject = AsyncCurrentValueSubject<IncomingPaymentStatus>(.detecting)
        subjects[groupId] = subject
        return subject
    }

    func startNextIfPossible() {
        while tasks.count < maxConcurrent, !pending.isEmpty {
            let next = pending.removeFirst()
            tasks[next.groupId] = next.run()
        }
    }
}
