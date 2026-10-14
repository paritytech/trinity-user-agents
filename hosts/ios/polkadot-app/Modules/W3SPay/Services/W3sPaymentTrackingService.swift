import Coinage
import CommonService
import Foundation
import SubstrateOperation
import SubstrateSdk

/// Tracks the on-chain lifecycle of W3S payments independently of any screen.
///
/// Subscribes to the payment history store and, for every record that still needs
/// tracking, verifies the send on-chain and then waits for the merchant claim,
/// persisting each transition. Because the CoreData subscription replays existing
/// records on setup, payments interrupted by a crash or app kill are reconciled
/// automatically on the next launch.
protocol W3sPaymentTracking: AsyncApplicationServicing {}

final class W3sPaymentTrackingService: W3sPaymentTracking, @unchecked Sendable {
    private let historyStore: W3sPaymentHistoryStoring
    private let sendVerifier: any TransferSendVerifying
    private let blockInfoProvider: BlockInfoProviding
    private let logger: LoggerProtocol

    private var subscription: Task<Void, Never>?
    private let taskRegistry = PaymentTaskRegistry()

    /// Statement-store expiry window in finalized blocks: the transferred coins
    /// either appear within it or never will.
    private static let sendWindowBlocks: UInt32 = 50
    /// Minimum probe for stale records: the storage subscription replays current
    /// state immediately, so a couple of blocks suffice to confirm presence.
    private static let minProbeBlocks: UInt32 = 2
    /// How long to wait for the merchant to claim before giving up for this run.
    /// The record stays `.sent`, so the claim watch resumes on next launch.
    private static let claimWaitBlocks: UInt32 = 100

    init(
        historyStore: W3sPaymentHistoryStoring,
        sendVerifier: any TransferSendVerifying,
        blockInfoProvider: BlockInfoProviding,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.historyStore = historyStore
        self.sendVerifier = sendVerifier
        self.blockInfoProvider = blockInfoProvider
        self.logger = logger
    }
}

extension W3sPaymentTrackingService {
    func setup() async {
        subscription = Task { [weak self] in
            guard let stream = self?.historyStore.observeAll() else { return }

            do {
                for try await records in stream {
                    try Task.checkCancellation()

                    for record in records where record.needsTracking {
                        await self?.trackIfNeeded(record)
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                self?.logger.error("Payment tracking subscription failed: \(error)")
            }
        }
    }

    func throttle() async {
        subscription?.cancel()
        subscription = nil
        await taskRegistry.cancelAll()
    }
}

private extension W3sPaymentTrackingService {
    func trackIfNeeded(_ record: W3sPaymentRecord) async {
        let paymentId = record.paymentId

        guard await taskRegistry.contains(paymentId) == false else { return }

        let task = Task { [weak self] in
            defer {
                Task { [weak self] in await self?.taskRegistry.remove(forPaymentId: paymentId) }
            }
            self?.logger
                .debug(
                    "Tracking \(record.paymentId) amount: \(record.amountString) entries: \(record.memo.entries.count) submittedAtBlock: \(record.submittedAtBlock.map(String.init) ?? "unknown")"
                )
            await self?.track(record)
        }

        await taskRegistry.register(task, forPaymentId: paymentId)
    }

    func track(_ record: W3sPaymentRecord) async {
        if record.status != .sent {
            let waitForClaim = await confirmSend(record)
            guard waitForClaim else {
                return
            }
        }
        await awaitClaim(record)
    }

    /// Confirms the send on-chain. Returns `true` when the claim watch should follow,
    /// `false` when tracking is done for this record (already claimed, cancelled, or failed).
    func confirmSend(_ record: W3sPaymentRecord) async -> Bool {
        logger.debug("Payment \(record.paymentId) awaiting send confirmation")
        do {
            switch try await sendVerifier.awaitSendOrClaimed(
                memo: record.memo,
                anchorBlock: anchorBlock(for: record),
                blockTimeout: remainingSendWindow(for: record)
            ) {
            case .onChain:
                try await historyStore.updateStatus(paymentId: record.paymentId, status: .sent)
                logger.debug("Payment \(record.paymentId) detected on-chain")
                return true
            case .alreadyClaimed:
                // Coins were sent and the merchant already claimed — skip the claim wait.
                try await historyStore.updateStatus(paymentId: record.paymentId, status: .claimed)
                logger.debug("Payment \(record.paymentId) already claimed on-chain")
                return false
            }
        } catch is CancellationError {
            return false
        } catch {
            await handleSendFailure(error, for: record)
            return false
        }
    }

    func awaitClaim(_ record: W3sPaymentRecord) async {
        logger.debug("Payment \(record.paymentId) awaiting claim")
        do {
            try await sendVerifier.awaitClaimOnChain(
                memo: record.memo,
                blockTimeout: Self.claimWaitBlocks
            )
            try await historyStore.updateStatus(paymentId: record.paymentId, status: .claimed)
            logger.debug("Payment \(record.paymentId) claimed by merchant")
        } catch is CancellationError {
        } catch {
            // Money is already on-chain — never mark failed here. The record stays
            // `.sent` and the claim watch resumes on the next launch.
            logger.error("Payment \(record.paymentId) claim wait failed: \(error)")
        }
    }

    /// Historical anchor for the send-or-claimed probe: the end of the send window relative to the
    /// submit-time finalized block. A coin claimed after the window is still present at this block,
    /// letting the tracker distinguish "claimed" from "never sent" on a later launch.
    func anchorBlock(for record: W3sPaymentRecord) -> BlockNumber? {
        record.submittedAtBlock.map { $0 + Self.sendWindowBlocks }
    }

    func remainingSendWindow(for record: W3sPaymentRecord) async -> UInt32 {
        guard
            let submittedAtBlock = record.submittedAtBlock,
            let currentBlock = try? await blockInfoProvider.fetchFinalized()
        else {
            return Self.sendWindowBlocks
        }

        let elapsed = currentBlock > submittedAtBlock ? currentBlock - submittedAtBlock : 0
        let remaining = Self.sendWindowBlocks - min(elapsed, Self.sendWindowBlocks)
        return max(remaining, Self.minProbeBlocks)
    }

    /// Marks the payment failed only when the send window has provably expired.
    /// Transient errors (RPC drops) leave the status untouched so tracking
    /// retries on the next launch.
    func handleSendFailure(_ error: Error, for record: W3sPaymentRecord) async {
        logger.error("Payment \(record.paymentId) send detection failed: \(error)")

        guard await isSendWindowExpired(for: record) else { return }

        do {
            try await historyStore.updateStatus(
                paymentId: record.paymentId,
                status: .failed(reason: "Coins not detected on-chain within send window")
            )
        } catch {
            logger.error("Payment \(record.paymentId) failed-status write failed: \(error)")
        }
    }

    func isSendWindowExpired(for record: W3sPaymentRecord) async -> Bool {
        guard let submittedAtBlock = record.submittedAtBlock else {
            // No block anchor: the windowed await above already ran its course.
            return true
        }

        guard let currentBlock = try? await blockInfoProvider.fetchFinalized() else {
            return false
        }

        return currentBlock > submittedAtBlock + Self.sendWindowBlocks
    }
}

private actor PaymentTaskRegistry {
    private var tasks: [String: Task<Void, Never>] = [:]

    func register(_ task: Task<Void, Never>, forPaymentId id: String) {
        tasks[id] = task
    }

    func remove(forPaymentId id: String) {
        tasks.removeValue(forKey: id)
    }

    func contains(_ id: String) -> Bool {
        tasks[id] != nil
    }

    func cancelAll() {
        tasks.values.forEach { $0.cancel() }
        tasks.removeAll()
    }
}

private extension W3sPaymentRecord {
    /// Whether the payment still requires on-chain lifecycle tracking.
    var needsTracking: Bool {
        switch status {
        case .pending,
             .submitted,
             .sent:
            true
        case .claimed,
             .failed,
             .revoked:
            false
        }
    }
}
