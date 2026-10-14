import BackgroundExecution
import ExtrinsicService
import Foundation
import SDKLogger
import SubstrateSdk

/// Builds every transaction waiting for its policy, in this process, as soon as it is committed.
///
/// It reacts to the ledger rather than to callers: a scheduled transaction is only acted on once a read
/// of the ledger shows it, so one scheduled inside a transaction that has not committed yet is simply
/// not seen until it has. That is what makes it safe for a caller to schedule from inside someone
/// else's open write.
///
/// Waiting transactions are grouped by policy and then by operation group, and each group is worked in
/// its own task — a policy waiting hours for a coin to appear never holds up another group.
public actor DurableSubmissionExecutor {
    /// The pacing of the executor and the clock it runs on. Injected so tests drive every backoff and
    /// cooldown from a test clock instead of waiting on the wall clock.
    public struct Timing: Sendable {
        /// How long a bucket waits after a round that decided nothing, doubling each time.
        let initialBackoff: Duration
        let maxBackoff: Duration

        /// How long a row waits before being built *again*, doubling with each rebuild. A transaction
        /// built again right after its last attempt failed would fail the same way just as fast if the
        /// failure repeats.
        let rebuildCooldown: Duration
        let maxRebuildCooldown: Duration

        let maxDoublings: Int
        let clock: any Clock<Duration>

        public static let production = Timing(
            initialBackoff: .seconds(2),
            maxBackoff: .seconds(120),
            rebuildCooldown: .seconds(5),
            maxRebuildCooldown: .seconds(600),
            maxDoublings: 10,
            clock: ContinuousClock()
        )

        public init(
            initialBackoff: Duration,
            maxBackoff: Duration,
            rebuildCooldown: Duration,
            maxRebuildCooldown: Duration,
            maxDoublings: Int,
            clock: any Clock<Duration>
        ) {
            self.initialBackoff = initialBackoff
            self.maxBackoff = maxBackoff
            self.rebuildCooldown = rebuildCooldown
            self.maxRebuildCooldown = maxRebuildCooldown
            self.maxDoublings = maxDoublings
            self.clock = clock
        }
    }

    fileprivate struct Bucket: Hashable, Sendable {
        let domainId: TxDomainId
        let policyId: SubmissionPolicyId
        let groupId: DurableTxGroupId?

        var logId: String { "domain=\(domainId) policy=\(policyId) group=\(groupId ?? "none")" }
    }

    private let store: any DurableTxRepositoryProtocol
    private let policies: DurableSubmissionPolicyRegistry
    private let launcher: any DurableAttemptStarting
    private let onPendingSubmissions: @Sendable () -> Void
    private let backgroundExecutor: any BackgroundExecuting
    private let timing: Timing
    private let logger: SDKLoggerProtocol?

    private var collector: Task<Void, Never>?

    /// Tells a collector that has just ended apart from one replaced since, so a stale task cannot
    /// clear a live one.
    private var collectorGeneration = 0

    private var buckets: [Bucket: Task<Void, Never>] = [:]

    /// In memory on purpose: a relaunch starting every transaction's cooldown over costs one early
    /// rebuild each, which is the cheaper mistake than persisting a counter nothing else reads.
    private var attemptsStarted: [DurableTxId: Int] = [:]

    public init(
        store: any DurableTxRepositoryProtocol,
        policies: DurableSubmissionPolicyRegistry,
        launcher: any DurableAttemptStarting,
        onPendingSubmissions: @escaping @Sendable () -> Void,
        backgroundExecutor: any BackgroundExecuting,
        timing: Timing = .production,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.policies = policies
        self.launcher = launcher
        self.onPendingSubmissions = onPendingSubmissions
        self.backgroundExecutor = backgroundExecutor
        self.timing = timing
        self.logger = logger
    }

    /// Starts watching the ledger for transactions to build. Idempotent.
    ///
    /// Keyed on there being no collector rather than on the last one being cancelled: a collector whose
    /// stream failed or finished returns normally, and such a task is *not* cancelled — so testing
    /// `isCancelled` would leave a dead collector in place and no-op for the rest of the process,
    /// stranding every scheduled transaction until relaunch.
    public func ensureStarted() {
        guard collector == nil else { return }

        collectorGeneration += 1
        let generation = collectorGeneration

        collector = Task { [weak self] in
            await self?.collectPendingSubmissions()
            await self?.collectorEnded(generation)
        }
    }

    /// Clears the handle of a collector that has ended, so the next ``ensureStarted()`` starts a new one.
    private func collectorEnded(_ generation: Int) {
        guard generation == collectorGeneration else { return }

        collector = nil
    }

    /// Cancels the collector and every bucket this instance owns.
    public func close() {
        collector?.cancel()
        collector = nil

        buckets.values.forEach { $0.cancel() }
        buckets.removeAll()
    }
}

// MARK: - Collecting

private extension DurableSubmissionExecutor {
    /// Every emission is acted on. A database query stream can skip the intermediate states between two
    /// emissions, so one that reads the same as the last may still hide a transaction that left and came
    /// back meanwhile.
    func collectPendingSubmissions() async {
        var lastPending: Set<DurableTxId> = []

        do {
            for try await pending in store.subscribePendingSubmissions() {
                guard !Task.isCancelled else { return }

                let ids = Set(pending.map(\.id))

                if !ids.isEmpty, ids != lastPending {
                    logger?.debug("Pending submissions count=\(ids.count)")
                    onPendingSubmissions()
                }

                lastPending = ids

                for bucket in Set(pending.map(bucket(of:))) {
                    launchIfIdle(bucket)
                }
            }
        } catch {
            logger?.error("Pending submission stream failed: \(error)")
        }
    }

    func bucket(of transaction: ScheduledDurableTx) -> Bucket {
        Bucket(
            domainId: transaction.domainId,
            policyId: transaction.policy.id,
            groupId: transaction.groupId
        )
    }

    func launchIfIdle(_ bucket: Bucket) {
        guard buckets[bucket]?.isCancelled ?? true else { return }

        buckets[bucket] = Task { [weak self] in
            await self?.runBucket(bucket)
        }
    }
}

// MARK: - Working one bucket

private extension DurableSubmissionExecutor {
    func runBucket(_ bucket: Bucket) async {
        guard let policy = policies.policy(for: bucket.policyId) else {
            // A row nothing can build would otherwise wait for ever.
            logger?.error("\(bucket.logId) submission skipped reason=no-registered-policy")
            await abandonAll(in: bucket)

            // Unconditionally, unlike the loop below: `abandonAll` swallows its own failure, so rows can
            // still be waiting here. Leaving the handle behind would keep this bucket permanently
            // "running" and stop it ever being launched again.
            buckets[bucket] = nil

            return
        }

        var failures = 0

        repeat {
            guard !Task.isCancelled else { return }

            if await workOnce(bucket, policy: policy) {
                failures = 0
            } else {
                failures += 1
                let backoff = delay(timing.initialBackoff, ceiling: timing.maxBackoff, doublings: failures - 1)
                logger?.debug("\(bucket.logId) backoff failures=\(failures) delay=\(backoff)")
                try? await timing.clock.sleep(for: backoff)
            }
        } while await !finishBucket(bucket)
    }

    /// Returns whether anything was decided, which is what separates progress from a loop worth backing
    /// off from.
    func workOnce(_ bucket: Bucket, policy: any DurableSubmissionPolicy) async -> Bool {
        let transactions: [ScheduledDurableTx]

        do {
            transactions = try await store.getPendingSubmissions(
                policyId: bucket.policyId,
                groupId: bucket.groupId
            )
        } catch {
            logger?.warning("\(bucket.logId) pending-submissions read failed: \(error)")

            return false
        }

        guard !transactions.isEmpty else { return true }

        // Deliberately outside the assertion below: a cooldown runs to ten minutes, far past the window
        // iOS grants, so holding one across it would spend the whole window waiting and then expire with
        // no work done.
        await awaitRebuildCooldown(bucket, transactions)

        do {
            return try await backgroundExecutor.execute {
                try await self.prepareAndStart(transactions, in: bucket, policy: policy)
            }
        } catch {
            logger?.warning("\(bucket.logId) prepare failed: \(error)")

            return false
        }
    }

    /// Building an extrinsic is the slow half — proofs, a pinned block, a reserved token — and none of it
    /// survives being suspended part-way: the next round starts over and the work is spent for nothing.
    /// So it runs under a background-task assertion, which the submission watch the launcher starts does
    /// not nest inside: that watch is an unstructured task with an assertion of its own, so this one
    /// expiring cannot cancel a submission already on the wire.
    func prepareAndStart(
        _ transactions: [ScheduledDurableTx],
        in bucket: Bucket,
        policy: any DurableSubmissionPolicy
    ) async throws -> Bool {
        let outcomes = try await policy.prepareSubmission(transactions)

        return await applyOutcomes(outcomes, for: transactions, in: bucket, chainId: policy.chainId)
    }

    func applyOutcomes(
        _ outcomes: [DurableTxId: SubmissionPreparation],
        for transactions: [ScheduledDurableTx],
        in bucket: Bucket,
        chainId: ChainId
    ) async -> Bool {
        let asked = Set(transactions.map(\.id))
        var decided = false

        for (id, outcome) in outcomes where asked.contains(id) {
            if await apply(outcome, to: id, in: bucket, chainId: chainId) {
                decided = true
            }
        }

        return decided
    }

    /// Returns whether `id` was decided: started, or failed for good.
    func apply(
        _ outcome: SubmissionPreparation,
        to id: DurableTxId,
        in bucket: Bucket,
        chainId: ChainId
    ) async -> Bool {
        switch outcome {
        case let .ready(model):
            return await start(model, for: id, in: bucket, chainId: chainId)
        case .giveUp:
            logger?.info("\(bucket.logId) entry=\(id) policy gave up")

            return await abandon(id)
        }
    }

    func start(
        _ model: ExtrinsicBuiltModel,
        for id: DurableTxId,
        in bucket: Bucket,
        chainId: ChainId
    ) async -> Bool {
        do {
            let started = try await launcher.startAttempt(id: id, model: model, chainId: chainId)

            if started {
                attemptsStarted[id] = (attemptsStarted[id] ?? 0) + 1
            }

            return started
        } catch let error as DurableTxError {
            // An extrinsic the engine cannot track would be rejected the same way on every rebuild.
            logger?.error("\(bucket.logId) entry=\(id) attempt rejected: \(error)")

            return await abandon(id)
        } catch {
            logger?.warning("\(bucket.logId) entry=\(id) attempt not started: \(error)")

            return false
        }
    }

    func abandon(_ id: DurableTxId) async -> Bool {
        do {
            return try await store.abandonSubmission(id: id)
        } catch {
            logger?.warning("Abandon failed id=\(id): \(error)")

            return false
        }
    }

    /// Every row naming this policy is unbuildable, whatever group it is in, so the store fails them
    /// all in one write rather than this reading them back and deciding one at a time.
    func abandonAll(in bucket: Bucket) async {
        do {
            let count = try await store.abandonSubmissions(
                domain: bucket.domainId,
                policyId: bucket.policyId
            )
            logger?.info("\(bucket.logId) abandoned=\(count) reason=no-registered-policy")
        } catch {
            logger?.warning("\(bucket.logId) abandon-all failed: \(error)")
        }
    }

    func awaitRebuildCooldown(_ bucket: Bucket, _ transactions: [ScheduledDurableTx]) async {
        let rebuilds = transactions.map { attemptsStarted[$0.id] ?? 0 }.max() ?? 0
        guard rebuilds > 0 else { return }

        let cooldown = delay(
            timing.rebuildCooldown,
            ceiling: timing.maxRebuildCooldown,
            doublings: rebuilds - 1
        )

        logger?.debug("\(bucket.logId) rebuild cooldown attempts=\(rebuilds) delay=\(cooldown)")
        try? await timing.clock.sleep(for: cooldown)
    }

    /// Ends the bucket's task unless something arrived for it meanwhile.
    func finishBucket(_ bucket: Bucket) async -> Bool {
        let stillWaiting: Bool

        do {
            stillWaiting = try await !store.getPendingSubmissions(
                policyId: bucket.policyId,
                groupId: bucket.groupId
            ).isEmpty
        } catch {
            stillWaiting = true
        }

        guard !stillWaiting else { return false }

        buckets[bucket] = nil

        return true
    }

    func delay(_ base: Duration, ceiling: Duration, doublings: Int) -> Duration {
        let factor = 1 << max(0, min(doublings, timing.maxDoublings))

        return min(base * factor, ceiling)
    }
}
