import AsyncExtensions
import BigInt
import Coinage
import Foundation
import NovaCrypto
import StructuredConcurrency
import SubstrateSdk
import TrUAPIHost

/// A trusted operation journal over the coordinator's native wallet, never a second inventory/allocator.
final class TrUAPINativeCoinage: NativeCoinageHost, @unchecked Sendable {
    struct Refusal: Error { let reason: NativeCoinageFailure }

    let wallet: Wallet
    let store: any NativeCoinageRecordStoring
    private let scope: @Sendable () throws -> NativeCoinageScope
    private let confirmationPresenter: TrUAPIConfirmationPresenting
    private let queue = SerialOperationQueue()
    private let lock = NSLock()
    private var available = false
    private var generation: UInt64 = 0
    private var leases: [UUID: NativeCoinageLease] = [:]
    private var reviews: [Data: Task<Bool?, Never>] = [:]

    convenience init(
        service: any CoinageServicing,
        storageFacade: StorageFacadeProtocol,
        scope: @escaping @Sendable () throws -> NativeCoinageScope,
        confirmationPresenter: TrUAPIConfirmationPresenting
    ) {
        self.init(
            wallet: Wallet(service: service),
            store: TrUAPINativeCoinageStore(storageFacade: storageFacade),
            scope: scope,
            confirmationPresenter: confirmationPresenter
        )
    }

    init(
        wallet: Wallet,
        store: any NativeCoinageRecordStoring,
        scope: @escaping @Sendable () throws -> NativeCoinageScope,
        confirmationPresenter: TrUAPIConfirmationPresenting
    ) {
        self.wallet = wallet
        self.store = store
        self.scope = scope
        self.confirmationPresenter = confirmationPresenter
    }

    /// Synchronous invalidation closes the gate before teardown starts, including callers suspended in UI/IO.
    func setAvailable(_ value: Bool) {
        let pending: [NativeCoinageLease] = lock.withLock {
            available = value
            generation &+= 1
            return Array(leases.values)
        }
        pending.forEach { $0.cancel() }
    }

    func nativeCoinage(request: NativeCoinageRequest) async throws -> NativeCoinageResponse {
        let id = UUID()
        let lease = lock.withLock {
            let lease = NativeCoinageLease(generation: generation)
            leases[id] = lease
            return lease
        }
        defer {
            lease.finish()
            _ = lock.withLock { leases.removeValue(forKey: id) }
        }
        return try await withTaskCancellationHandler {
            do {
                var step = try await queued(lease) { [self] in try await perform(request, lease: lease) }
                // The user reviews outside the queue, so an open payment sheet never blocks other operations.
                while case let .review(pending) = step {
                    guard let approved = await decision(for: pending, lease: lease) else {
                        throw Refusal(reason: .unavailable)
                    }
                    step = try await queued(lease) { [self] in
                        try await completeReview(pending, approved: approved, lease: lease)
                    }
                }
                guard case let .done(response) = step else { throw Refusal(reason: .unavailable) }
                return response
            } catch let error as Refusal {
                return .failed(reason: error.reason)
            } catch is CancellationError {
                return .failed(reason: .unavailable)
            } catch {
                // No native error description crosses the boundary: backend errors can contain keys.
                throw HostRejection.Rejected(reason: "Native Coinage operation unavailable")
            }
        } onCancel: {
            lease.cancel()
        }
    }

    func check(_ binding: NativeCoinageBinding, _ lease: NativeCoinageLease) throws {
        try Task.checkCancellation()
        guard lock.withLock({ available && generation == lease.generation }), !lease.isCancelled else {
            throw Refusal(reason: .unavailable)
        }
        let current: NativeCoinageScope
        do { current = try scope() } catch { throw Refusal(reason: .unavailable) }
        guard binding == NativeCoinageBinding(current) else { throw Refusal(reason: .invalidRequest) }
    }

    private func perform(
        _ request: NativeCoinageRequest,
        lease: NativeCoinageLease
    ) async throws -> Step {
        let binding = NativeCoinageBinding(request.scope)
        guard binding.root.count == 32, binding.genesis.count == 32 else {
            throw Refusal(reason: .invalidRequest)
        }
        try check(binding, lease)
        if case .denomination = request.operation {
            return try await .done(denomination(binding: binding, lease: lease))
        }
        let records = try await store.records(binding: binding)
        try check(binding, lease)
        if case let .preparePayment(intent) = request.operation {
            return try await prepare(intent, binding: binding, records: records, lease: lease)
        }
        return try await .done(dispatch(request.operation, binding: binding, records: records, lease: lease))
    }

    private func dispatch(
        _ operation: NativeCoinageOperation,
        binding: NativeCoinageBinding,
        records: [NativeCoinageRecord],
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        switch operation {
        case .denomination, .preparePayment:
            throw Refusal(reason: .invalidRequest)
        case let .commitHandoff(productId, operationId):
            return try await markAccepted(
                records, product: productId, operation: operationId, delivered: false, binding: binding, lease: lease
            )
        case let .noteDelivery(productId, operationId):
            // Authenticated peer delivery proves transport custody even if CommitHandoff was lost.
            return try await markAccepted(
                records, product: productId, operation: operationId, delivered: true, binding: binding, lease: lease
            )
        case let .readHandoff(productId, operationId):
            return try await readHandoff(
                records, product: productId, operation: operationId, binding: binding, lease: lease
            )
        case let .views(productId):
            return try await payments(records, product: productId, lease: lease)
        case let .pendingHandoffs(productId, acceptedOperations):
            return try await pendingHandoffs(
                records, product: productId, accepted: Set(acceptedOperations), binding: binding, lease: lease
            )
        case .reconcile:
            // Engine/IncomingPaymentService own transaction recovery. Read their actual status, never spend anew.
            for case let .outgoing(record) in records where record.approval == .approved {
                _ = try await card(record, lease: lease)
            }
            try check(binding, lease)
            return .done
        case let .topUp(productId, operationId, minimumAmountRaw, secretKeys):
            return try await topUp(
                TopUpRequest(
                    product: productId,
                    operation: operationId,
                    minimum: minimumAmountRaw,
                    secrets: secretKeys
                ),
                binding: binding,
                records: records,
                lease: lease
            )
        }
    }

    private func denomination(
        binding: NativeCoinageBinding,
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        let context = try await wallet.denomination()
        try check(binding, lease)
        let unit = context.valueInPlanks(for: 0)
        guard unit > 0, unit.bitWidth <= 128 else { throw Refusal(reason: .unavailable) }
        return .denomination(centsUnitRaw: String(unit))
    }

    private func markAccepted(
        _ records: [NativeCoinageRecord],
        product: String,
        operation: Data,
        delivered: Bool,
        binding: NativeCoinageBinding,
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        var record = try outgoing(records, product: product, operation: operation)
        guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
        guard try await wallet.retained(record.custodyId) != nil else { throw Refusal(reason: .operationNotFound) }
        try check(binding, lease)
        record.accepted = true
        if delivered { record.delivered = true }
        try await store.save(.outgoing(record)) { [self] in try check(binding, lease) }
        try check(binding, lease)
        return .done
    }

    private func readHandoff(
        _ records: [NativeCoinageRecord],
        product: String,
        operation: Data,
        binding: NativeCoinageBinding,
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        let record = try outgoing(records, product: product, operation: operation)
        guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
        guard let memo = try await wallet.retained(record.custodyId) else {
            throw Refusal(reason: .operationNotFound)
        }
        try check(binding, lease)
        return try await prepared(record, memo: memo, lease: lease, requireHandoff: true)
    }

    private func pendingHandoffs(
        _ records: [NativeCoinageRecord],
        product: String,
        accepted: Set<Data>,
        binding: NativeCoinageBinding,
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        var records = records
        for index in records.indices {
            guard case var .outgoing(record) = records[index], record.intent.product == product,
                  accepted.contains(record.intent.operation), record.approval == .approved else { continue }
            guard try await wallet.retained(record.custodyId) != nil else { continue }
            try check(binding, lease)
            record.accepted = true
            try await store.save(.outgoing(record)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            records[index] = .outgoing(record)
        }
        let pending = records.filter {
            guard case let .outgoing(record) = $0 else { return false }
            return record.accepted && !record.delivered && accepted.contains(record.intent.operation)
        }
        return try await payments(pending, product: product, lease: lease, onlyPending: true)
    }

    func outgoing(
        _ records: [NativeCoinageRecord],
        product: String,
        operation: Data
    ) throws -> NativeCoinageOutgoing {
        guard let existing = records.first(where: { $0.operation == operation }) else {
            throw Refusal(reason: .operationNotFound)
        }
        guard case let .outgoing(record) = existing, record.intent.product == product else {
            throw Refusal(reason: .operationConflict)
        }
        return record
    }
}

extension TrUAPINativeCoinage {
    private func queued(
        _ lease: NativeCoinageLease,
        _ body: @escaping @Sendable () async throws -> Step
    ) async throws -> Step {
        try await queue.run {
            let operation = Task { try await body() }
            lease.onCancel { operation.cancel() }
            return try await operation.value
        }
    }

    /// One user decision per operation, shared by concurrent retries; `nil` when the prompt was cancelled.
    private func decision(for pending: PendingReview, lease: NativeCoinageLease) async -> Bool? {
        let operation = pending.record.intent.operation
        let review = lock.withLock {
            if let existing = reviews[operation] { return existing }
            let task = Task { [confirmationPresenter] () -> Bool? in
                let approved = await confirmationPresenter.confirmNativeCoinage(
                    review: pending.review,
                    requiresPrivacyConfirmation: pending.privacy
                )
                return Task.isCancelled ? nil : approved
            }
            reviews[operation] = task
            return task
        }
        lease.onCancel { review.cancel() }
        let approved = await review.value
        lock.withLock {
            if reviews[operation] == review { reviews[operation] = nil }
        }
        return approved
    }

    static func recipientAmount(_ selection: CoinSelectionResult, context: DenominationBreakdownContext) -> BigUInt {
        switch selection {
        case let .exactMatch(coins):
            coins.reduce(0) { $0 + context.valueInPlanks(for: $1.exponent) }
        case let .split(whole, _, target, _):
            whole.reduce(0) { $0 + context.valueInPlanks(for: $1.exponent) }
                + target.reduce(0) { $0 + context.valueInPlanks(for: $1.exponent) }
        case let .unloadIntoCoins(coins, groups):
            coins.reduce(0) { $0 + context.valueInPlanks(for: $1.exponent) }
                + groups.flatMap(\.recipientDenominations).reduce(0) { $0 + context.valueInPlanks(for: $1.exponent) }
        }
    }

    func prepared(
        _ record: NativeCoinageOutgoing, memo: TransferMemo, lease: NativeCoinageLease, requireHandoff: Bool = false
    ) async throws -> NativeCoinageResponse {
        guard let unit = BigUInt(record.centsUnit), memo.totalValue == unit * BigUInt(record.intent.cents),
              !memo.entries.isEmpty, memo.entries.allSatisfy({ $0.count == 64 }),
              Set(memo.entries).count == memo.entries.count else { throw Refusal(reason: .operationConflict) }
        let payment = try await card(record, memo: memo, lease: lease)
        try check(record.binding, lease)
        switch payment.state {
        case .delivered, .cleared, .failed:
            guard !requireHandoff else { throw Refusal(reason: .operationNotFound) }
            return .prepared(payment: payment, memo: nil)
        default:
            break
        }
        return .prepared(
            payment: payment,
            memo: NativeCoinageMemo(
                secretKeys: memo.entries,
                totalValueRaw: String(memo.totalValue)
            )
        )
    }

    private func payments(
        _ records: [NativeCoinageRecord], product: String, lease: NativeCoinageLease, onlyPending: Bool = false
    ) async throws -> NativeCoinageResponse {
        var result: [HostNativeChatPayment] = []
        for case let .outgoing(record) in records where record.intent.product == product {
            let value = try await card(record, lease: lease)
            if onlyPending {
                switch value.state {
                case .cleared, .failed: continue
                default: break
                }
            }
            result.append(value)
        }
        result.sort { ($0.timestamp, $0.messageId) < ($1.timestamp, $1.messageId) }
        return .payments(payments: result)
    }

    private func card(
        _ record: NativeCoinageOutgoing, memo supplied: TransferMemo? = nil, lease: NativeCoinageLease
    ) async throws -> HostNativeChatPayment {
        let memo: TransferMemo?
        if let supplied { memo = supplied } else { memo = try await wallet.retained(record.custodyId) }
        try check(record.binding, lease)
        var state: HostNativeChatPaymentState
        if record.delivered {
            state = .delivered
        } else if record.accepted {
            state = .delivering
        } else {
            state = .preparing
        }
        if record.approval == .rejected {
            state = .failed(reason: .cancelled)
        } else if let memo {
            let statuses = try await wallet.statuses(memo.entries)
            try check(record.binding, lease)
            let keys = try memo.entries.map { try SNKeyFactory().createPublicKey(fromSecret: $0).rawData() }
            let owned = keys.compactMap { statuses[$0] }
            if owned.count == keys.count, owned.allSatisfy({ $0.status == .claimed(finalized: true) }) {
                state = .cleared
            } else if owned.count == keys.count, owned.allSatisfy({ $0.status == .failed }) {
                state = .failed(reason: .chainRejected)
            } else {
                let context = try await wallet.denomination()
                try check(record.binding, lease)
                let cleared = owned.filter { $0.status == .claimed(finalized: true) }
                    .reduce(BigUInt(0)) { $0 + context.valueInPlanks(for: $1.coin.exponent) }
                if let unit = BigUInt(record.centsUnit), unit > 0, cleared > 0,
                   let cents = UInt64(exactly: cleared / unit), cents > 0, cents < record.intent.cents {
                    state = .partiallyCleared(clearedCents: cents)
                }
            }
        }
        return HostNativeChatPayment(
            operationId: record.intent.operation,
            requestId: record.intent.request,
            messageId: "payment-\(record.intent.operation.toHex())",
            timestamp: record.timestamp,
            peerIdentity: record.intent.peer,
            direction: .outgoing,
            amountCents: record.intent.cents,
            state: state
        )
    }

    static func topUpOutcome(_ status: IncomingPaymentStatus) -> NativeCoinageTopUpOutcome {
        switch status {
        case .claimed(finalized: true): .cleared
        case let .claimedPartially(actualClaimed):
            actualClaimed == 0 ? .notClaimed : .partial(creditedAmountRaw: String(actualClaimed))
        case .notClaimed: .notClaimed
        case .detecting, .claiming, .claimed(finalized: false): .pending
        }
    }
}

final class NativeCoinageLease: @unchecked Sendable {
    let generation: UInt64
    private let lock = NSLock()
    private var cancelled = false
    private var cancellation: (@Sendable () -> Void)?

    init(generation: UInt64) { self.generation = generation }
    var isCancelled: Bool { lock.withLock { cancelled } }
    func onCancel(_ action: @escaping @Sendable () -> Void) {
        let run = lock.withLock { cancellation = action; return cancelled }
        if run { action() }
    }
    func cancel() {
        let action = lock.withLock { cancelled = true; return cancellation }
        action?()
    }
    func finish() { lock.withLock { cancellation = nil } }
}
