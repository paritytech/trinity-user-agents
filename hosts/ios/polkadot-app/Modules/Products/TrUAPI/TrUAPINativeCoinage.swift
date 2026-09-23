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
    /// Narrow service seam used by the regression harness; production always wraps the coordinator's service.
    struct Wallet: @unchecked Sendable {
        let denomination: () async throws -> DenominationBreakdownContext
        let preview: (BigUInt) async throws -> TransferPreview
        let prepare: (CoinSelectionResult, String, @escaping @Sendable () throws -> Void) async throws -> TransferMemo
        let retained: (String) async throws -> TransferMemo?
        /// Matches native subscribeStatuses: INPUT memo secrets; OUTPUT keyed by their derived public keys.
        let statuses: ([Data]) async throws -> [Data: CoinageTransferState]
        let accept: (BigUInt, [Data], String, String) async throws -> Void
        let incomingStatus: (String, String) async throws -> IncomingPaymentStatus

        init(service: any CoinageServicing) {
            denomination = { try await service.denominationContext() }
            preview = { try await service.previewTransfer(for: $0) }
            prepare = { selection, id, authorization in
                // The native execute overload retains custody atomically with tx registration.
                // Its memo cannot escape while merely provisional, even if Host transport has not accepted it.
                try await service.executeTransfer(
                    result: selection, groupId: id, custodyId: id, authorization: authorization
                ).memo
            }
            retained = { try await service.retainedTransfer(custodyId: $0) }
            statuses = { keys in
                for try await snapshot in service.transferStatusService.subscribeStatuses(coinKeys: keys) {
                    return snapshot
                }
                throw CancellationError()
            }
            accept = { amount, keys, id, product in
                try await service.incomingPaymentService.accept(
                    amount: amount, descriptor: .coins(secretKeys: keys), paymentId: id, productId: product
                )
            }
            incomingStatus = { id, product in
                let stream = try await service.incomingPaymentService.subscribeStatus(for: id, productId: product)
                for try await status in stream { return status }
                throw CancellationError()
            }
        }

        init(
            denomination: @escaping () async throws -> DenominationBreakdownContext,
            preview: @escaping (BigUInt) async throws -> TransferPreview,
            prepare: @escaping (CoinSelectionResult, String, @escaping @Sendable () throws -> Void) async throws -> TransferMemo,
            retained: @escaping (String) async throws -> TransferMemo?,
            statuses: @escaping ([Data]) async throws -> [Data: CoinageTransferState],
            accept: @escaping (BigUInt, [Data], String, String) async throws -> Void,
            incomingStatus: @escaping (String, String) async throws -> IncomingPaymentStatus
        ) {
            self.denomination = denomination
            self.preview = preview
            self.prepare = prepare
            self.retained = retained
            self.statuses = statuses
            self.accept = accept
            self.incomingStatus = incomingStatus
        }
    }

    private struct Refusal: Error { let reason: NativeCoinageFailure }
    private let wallet: Wallet
    private let store: any NativeCoinageRecordStoring
    private let scope: @Sendable () throws -> NativeCoinageScope
    private let confirmationPresenter: TrUAPIConfirmationPresenting
    private let queue = SerialOperationQueue()
    private let lock = NSLock()
    private var available = false
    private var generation: UInt64 = 0
    private var leases: [UUID: NativeCoinageLease] = [:]

    convenience init(
        service: any CoinageServicing,
        storageFacade: StorageFacadeProtocol,
        scope: @escaping @Sendable () throws -> NativeCoinageScope,
        confirmationPresenter: TrUAPIConfirmationPresenting
    ) {
        self.init(wallet: Wallet(service: service), store: TrUAPINativeCoinageStore(storageFacade: storageFacade),
                  scope: scope, confirmationPresenter: confirmationPresenter)
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
                return try await queue.run { [self] in
                    let operation = Task { try await perform(request, lease: lease) }
                    lease.onCancel { operation.cancel() }
                    return try await operation.value
                }
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

    private func check(_ binding: NativeCoinageBinding, _ lease: NativeCoinageLease) throws {
        try Task.checkCancellation()
        guard lock.withLock({ available && generation == lease.generation }), !lease.isCancelled else {
            throw Refusal(reason: .unavailable)
        }
        let current: NativeCoinageScope
        do { current = try scope() } catch { throw Refusal(reason: .unavailable) }
        guard binding == NativeCoinageBinding(current) else { throw Refusal(reason: .invalidRequest) }
    }

    private func perform(_ request: NativeCoinageRequest, lease: NativeCoinageLease) async throws -> NativeCoinageResponse {
        let binding = NativeCoinageBinding(request.scope)
        guard binding.root.count == 32, binding.genesis.count == 32 else { throw Refusal(reason: .invalidRequest) }
        try check(binding, lease)
        if case .denomination = request.operation {
            let context = try await wallet.denomination()
            try check(binding, lease)
            let unit = context.valueInPlanks(for: 0)
            guard unit > 0, unit.bitWidth <= 128 else { throw Refusal(reason: .unavailable) }
            return .denomination(centsUnitRaw: String(unit))
        }
        var records = try await store.records(binding: binding)
        try check(binding, lease)
        switch request.operation {
        case .denomination:
            throw Refusal(reason: .invalidRequest)
        case let .preparePayment(intent):
            return try await prepare(intent, binding: binding, records: records, lease: lease)
        case let .commitHandoff(productId, operationId):
            var record = try outgoing(records, product: productId, operation: operationId)
            guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
            guard try await wallet.retained(record.custodyId) != nil else { throw Refusal(reason: .operationNotFound) }
            try check(binding, lease)
            record.accepted = true
            try await store.save(.outgoing(record)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            return .done
        case let .noteDelivery(productId, operationId):
            var record = try outgoing(records, product: productId, operation: operationId)
            guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
            guard try await wallet.retained(record.custodyId) != nil else {
                throw Refusal(reason: .operationNotFound)
            }
            try check(binding, lease)
            // Authenticated peer delivery proves transport custody even if CommitHandoff was lost.
            record.accepted = true
            record.delivered = true
            try await store.save(.outgoing(record)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            return .done
        case let .readHandoff(productId, operationId):
            let record = try outgoing(records, product: productId, operation: operationId)
            guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
            guard let memo = try await wallet.retained(record.custodyId) else {
                throw Refusal(reason: .operationNotFound)
            }
            try check(binding, lease)
            return try await prepared(record, memo: memo, lease: lease, requireHandoff: true)
        case let .views(productId):
            return try await payments(records, product: productId, lease: lease)
        case let .pendingHandoffs(productId, acceptedOperations):
            let accepted = Set(acceptedOperations)
            for index in records.indices {
                guard case var .outgoing(record) = records[index], record.intent.product == productId,
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
            return try await payments(pending, product: productId, lease: lease, onlyPending: true)
        case .reconcile:
            // Engine/IncomingPaymentService own transaction recovery. Read their actual status, never spend anew.
            for case let .outgoing(record) in records where record.approval == .approved {
                _ = try await card(record, lease: lease)
            }
            try check(binding, lease)
            return .done
        case let .topUp(productId, operationId, minimumAmountRaw, secretKeys):
            return try await topUp(product: productId, operation: operationId, minimum: minimumAmountRaw,
                                   secrets: secretKeys, binding: binding, records: records, lease: lease)
        }
    }

    private func outgoing(_ records: [NativeCoinageRecord], product: String, operation: Data) throws -> NativeCoinageOutgoing {
        guard let existing = records.first(where: { $0.operation == operation }) else {
            throw Refusal(reason: .operationNotFound)
        }
        guard case let .outgoing(record) = existing, record.intent.product == product else {
            throw Refusal(reason: .operationConflict)
        }
        return record
    }

    private func prepare(
        _ intent: NativeCoinagePaymentIntent,
        binding: NativeCoinageBinding,
        records: [NativeCoinageRecord],
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        guard intent.operationId.count == 32, intent.peerIdentity.count == 32, !intent.productId.isEmpty,
              !intent.requestId.isEmpty, intent.amountCents > 0 else { throw Refusal(reason: .invalidRequest) }
        let immutable = NativeCoinageIntent(intent)
        for case let .outgoing(existing) in records
            where existing.intent.product == intent.productId && existing.intent.request == intent.requestId {
            guard existing.intent == immutable else { throw Refusal(reason: .operationConflict) }
        }
        var record: NativeCoinageOutgoing?
        if records.contains(where: { $0.operation == intent.operationId }) {
            record = try outgoing(records, product: intent.productId, operation: intent.operationId)
            guard record?.intent == immutable else { throw Refusal(reason: .operationConflict) }
            if record?.approval == .rejected { throw Refusal(reason: .userRejected) }
        }
        if let record, let memo = try await wallet.retained(record.custodyId) {
            try check(binding, lease)
            guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
            return try await prepared(record, memo: memo, lease: lease)
        }
        try check(binding, lease)
        let context = try await wallet.denomination()
        try check(binding, lease)
        let unit = context.valueInPlanks(for: 0)
        let amount = unit * BigUInt(intent.amountCents)
        guard unit > 0, amount.bitWidth <= 128 else { throw Refusal(reason: .invalidRequest) }
        if let record, record.centsUnit != String(unit) { throw Refusal(reason: .operationConflict) }
        let preview: TransferPreview
        do { preview = try await wallet.preview(amount) }
        catch CoinSelectionError.insufficientFunds, CoinSelectionError.emptyWallet {
            throw Refusal(reason: .insufficientBalance)
        }
        try check(binding, lease)
        guard preview.fullAmount == amount, Self.recipientAmount(preview.selectionResult, context: context) == amount else {
            throw Refusal(reason: .operationConflict)
        }
        var value = record ?? NativeCoinageOutgoing(
            binding: binding, intent: immutable, timestamp: UInt64(Date().timeIntervalSince1970 * 1000),
            centsUnit: String(unit), approval: .reviewing, privacyApproved: false, accepted: false, delivered: false
        )
        let privacy = preview.scope == .withConfirmation
        if value.approval != .approved || (privacy && !value.privacyApproved) {
            try await store.save(.outgoing(value)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            let approved = await confirmationPresenter.confirmNativeCoinage(
                review: MainPurseChatPaymentReview(
                    callingProductId: intent.productId, recipientIdentity: intent.peerIdentity,
                    recipientUsername: intent.recipientUsername, amountCents: intent.amountCents,
                    maxDebitCents: intent.amountCents, genesisHash: binding.genesis,
                    coinageInstanceId: binding.instance, operationId: intent.operationId
                ),
                requiresPrivacyConfirmation: privacy
            )
            try check(binding, lease)
            value.approval = approved ? .approved : .rejected
            value.privacyApproved = approved && privacy
            try await store.save(.outgoing(value)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            guard approved else { throw Refusal(reason: .userRejected) }
        }
        // Native registration + retention is one transaction. No memo is persisted by this adapter.
        let memo = try await wallet.prepare(preview.selectionResult, value.custodyId) { [self] in
            try check(binding, lease)
        }
        try check(binding, lease)
        return try await prepared(value, memo: memo, lease: lease)
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

    private func prepared(
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
        return .prepared(payment: payment, memo: NativeCoinageMemo(secretKeys: memo.entries, totalValueRaw: String(memo.totalValue)))
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
        var state: HostNativeChatPaymentState = record.delivered ? .delivered : (record.accepted ? .delivering : .preparing)
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
            operationId: record.intent.operation, requestId: record.intent.request,
            messageId: "payment-\(record.intent.operation.toHex())", timestamp: record.timestamp,
            peerIdentity: record.intent.peer, direction: .outgoing, amountCents: record.intent.cents, state: state
        )
    }

    private func topUp(
        product: String, operation: Data, minimum: String, secrets: [Data], binding: NativeCoinageBinding,
        records: [NativeCoinageRecord], lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        guard !product.isEmpty, operation.count == 32, let amount = BigUInt(minimum),
              amount.bitWidth <= 128, String(amount) == minimum else { throw Refusal(reason: .invalidRequest) }
        guard !secrets.isEmpty, secrets.allSatisfy({ $0.count == 64 }) else { throw Refusal(reason: .invalidSource) }
        let pairs: [(Data, Data)]
        do {
            pairs = try secrets.map { (try SNKeyFactory().createPublicKey(fromSecret: $0).rawData(), $0) }
                .sorted { $0.0.lexicographicallyPrecedes($1.0) }
        } catch { throw Refusal(reason: .invalidSource) }
        let sources = pairs.map(\.0)
        guard Set(sources).count == sources.count else { throw Refusal(reason: .invalidSource) }
        let incoming = NativeCoinageIncoming(binding: binding, operation: operation, product: product,
                                            minimum: minimum, sources: sources)
        if let prior = records.first(where: { $0.operation == operation }) {
            guard prior == .incoming(incoming) else { throw Refusal(reason: .operationConflict) }
        } else {
            for case let .incoming(prior) in records where !Set(prior.sources).isDisjoint(with: Set(sources)) {
                throw Refusal(reason: .operationConflict)
            }
            // Immutable min and source identity survive IncomingPaymentService's terminal secret cleanup.
            try await store.save(.incoming(incoming)) { [self] in try check(binding, lease) }
            try check(binding, lease)
        }
        let paymentId = "truapi-native-\(binding.key)-\(operation.toHex())"
        do {
            try await wallet.accept(amount, pairs.map(\.1), paymentId, product)
        } catch IncomingPaymentError.alreadyExists {
            // Only legal after our durable immutable binding matched above.
        } catch IncomingPaymentError.invalidAmount {
            throw Refusal(reason: .invalidRequest)
        } catch IncomingPaymentError.invalidSource {
            throw Refusal(reason: .invalidSource)
        } catch IncomingPaymentError.sourceBusy {
            throw Refusal(reason: .operationConflict)
        }
        try check(binding, lease)
        let status = try await wallet.incomingStatus(paymentId, product)
        try check(binding, lease)
        if case let .claimedPartially(actual) = status, actual >= amount {
            throw Refusal(reason: .operationConflict)
        }
        return .topUp(outcome: Self.topUpOutcome(status))
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

private final class NativeCoinageLease: @unchecked Sendable {
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
