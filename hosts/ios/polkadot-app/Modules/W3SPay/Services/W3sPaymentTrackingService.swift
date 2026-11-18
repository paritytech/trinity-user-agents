import Coinage
import CommonService
import Foundation

/// Tracks the on-chain lifecycle of W3S payments independently of any screen.
///
/// Subscribes to the payment history store and, for every record that still needs
/// tracking, follows what became of the coins of the payment's export, persisting each
/// transition. Because the CoreData subscription replays existing records on setup,
/// payments interrupted by a crash or app kill are reconciled automatically on the next launch.
protocol W3sPaymentTracking: AsyncApplicationServicing {}

final class W3sPaymentTrackingService: W3sPaymentTracking, @unchecked Sendable {
    private let historyStore: W3sPaymentHistoryStoring
    private let coinageService: any CoinageServicing
    private let logger: LoggerProtocol

    private var subscription: Task<Void, Never>?
    private let taskRegistry = PaymentTaskRegistry()

    init(
        historyStore: W3sPaymentHistoryStoring,
        coinageService: any CoinageServicing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.historyStore = historyStore
        self.coinageService = coinageService
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
            self?.logger.debug(
                "Tracking \(record.paymentId) amount: \(record.amountString) entries: \(record.memo.entries.count)"
            )
            await self?.track(record)
        }

        await taskRegistry.register(task, forPaymentId: paymentId)
    }

    /// Follows the export until every coin is settled; a payment the wallet cannot follow keeps its status
    /// and is looked at again on the next launch.
    func track(_ record: W3sPaymentRecord) async {
        do {
            let exportId = try W3sPaymentRecord.exportId(paymentId: record.paymentId)
            var lastStatus = record.status

            for try await coins in coinageService.subscribePaymentStates(exportId: exportId) {
                if let status = coins.paymentRecordStatus, status != lastStatus {
                    try await historyStore.updateStatus(paymentId: record.paymentId, status: status)
                    logger.debug("Payment \(record.paymentId) is now \(status)")
                    lastStatus = status
                }
                if !coins.isEmpty, coins.allSatisfy(\.status.isTerminal) { break }
            }
        } catch is CancellationError {
        } catch {
            logger.error("Payment \(record.paymentId) could not be followed: \(error)")
        }
    }
}

private extension [CoinagePaymentState] {
    /// `nil` while no coin is on chain yet, which leaves the record as the submitter wrote it.
    var paymentRecordStatus: W3sPaymentRecord.Status? {
        guard !isEmpty else { return nil }

        let claimed = filter { if case .claimed = $0.status { true } else { false } }
        let awaiting = filter { $0.status == .awaitingClaim }

        if allSatisfy(\.status.isTerminal) {
            return claimed.isEmpty ? .failed(reason: "The coins were never minted or were taken back") : .claimed
        }
        return awaiting.isEmpty && claimed.isEmpty ? nil : .sent
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
