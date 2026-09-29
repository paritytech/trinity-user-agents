import BigInt
import Coinage
import Foundation
import TrUAPIHost

/// Main-purse payment preparation. The user reviews outside the serial queue; approval covers the
/// recipient, amount and operation, never specific coins, so coins are reselected after review.
extension TrUAPINativeCoinage {
    /// One queued step. A payment that needs review leaves the queue while the user decides.
    enum Step: Sendable {
        case done(NativeCoinageResponse)
        case review(PendingReview)
    }

    /// A durably reviewing payment whose decision is awaited outside the serial queue.
    struct PendingReview: @unchecked Sendable {
        let record: NativeCoinageOutgoing
        let review: MainPurseChatPaymentReview
        let privacy: Bool
    }

    private struct Quote {
        let unit: BigUInt
        let selection: CoinSelectionResult
        let privacy: Bool
    }

    func prepare(
        _ intent: NativeCoinagePaymentIntent,
        binding: NativeCoinageBinding,
        records: [NativeCoinageRecord],
        lease: NativeCoinageLease
    ) async throws -> Step {
        try Self.validate(intent)
        let immutable = NativeCoinageIntent(intent)
        let record = try existingOutgoing(for: intent, immutable: immutable, records: records)
        if let record, let memo = try await wallet.retained(record.custodyId) {
            try check(binding, lease)
            guard record.approval == .approved else { throw Refusal(reason: .operationConflict) }
            return try await .done(prepared(record, memo: memo, lease: lease))
        }
        try check(binding, lease)
        let quote = try await quote(cents: intent.amountCents, record: record, binding: binding, lease: lease)
        let value = record ?? NativeCoinageOutgoing(
            binding: binding,
            intent: immutable,
            timestamp: UInt64(Date().timeIntervalSince1970 * 1000),
            centsUnit: String(quote.unit),
            approval: .reviewing,
            privacyApproved: false,
            accepted: false,
            delivered: false
        )
        return try await advance(value, quote: quote, lease: lease)
    }

    /// Record the user's decision on the still-unchanged operation, then prepare from a fresh selection.
    func completeReview(
        _ pending: PendingReview,
        approved: Bool,
        lease: NativeCoinageLease
    ) async throws -> Step {
        let binding = pending.record.binding
        try check(binding, lease)
        let records = try await store.records(binding: binding)
        try check(binding, lease)
        var value = try outgoing(
            records, product: pending.record.intent.product, operation: pending.record.intent.operation
        )
        guard value.intent == pending.record.intent, value.centsUnit == pending.record.centsUnit else {
            throw Refusal(reason: .operationConflict)
        }
        guard value.approval != .rejected else { throw Refusal(reason: .userRejected) }
        if Self.needsReview(value, privacy: pending.privacy) {
            value.approval = approved ? .approved : .rejected
            value.privacyApproved = approved && pending.privacy
            try await store.save(.outgoing(value)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            guard approved else { throw Refusal(reason: .userRejected) }
        }
        if let memo = try await wallet.retained(value.custodyId) {
            try check(binding, lease)
            return try await .done(prepared(value, memo: memo, lease: lease))
        }
        // Another operation may have spent the reviewed coins while the queue was released.
        let quote = try await quote(cents: value.intent.cents, record: value, binding: binding, lease: lease)
        return try await advance(value, quote: quote, lease: lease)
    }

    private func advance(
        _ value: NativeCoinageOutgoing,
        quote: Quote,
        lease: NativeCoinageLease
    ) async throws -> Step {
        let binding = value.binding
        if Self.needsReview(value, privacy: quote.privacy) {
            try await store.save(.outgoing(value)) { [self] in try check(binding, lease) }
            try check(binding, lease)
            return .review(PendingReview(
                record: value,
                review: MainPurseChatPaymentReview(
                    callingProductId: value.intent.product,
                    recipientIdentity: value.intent.peer,
                    recipientUsername: value.intent.username,
                    amountCents: value.intent.cents,
                    maxDebitCents: value.intent.cents,
                    genesisHash: binding.genesis,
                    coinageInstanceId: binding.instance,
                    operationId: value.intent.operation
                ),
                privacy: quote.privacy
            ))
        }
        // Native registration + retention is one transaction. No memo is persisted by this adapter.
        let memo = try await wallet.prepare(quote.selection, value.custodyId) { [self] in
            try check(binding, lease)
        }
        try check(binding, lease)
        return try await .done(prepared(value, memo: memo, lease: lease))
    }

    private func quote(
        cents: UInt64,
        record: NativeCoinageOutgoing?,
        binding: NativeCoinageBinding,
        lease: NativeCoinageLease
    ) async throws -> Quote {
        let context = try await wallet.denomination()
        try check(binding, lease)
        let unit = context.valueInPlanks(for: 0)
        let amount = unit * BigUInt(cents)
        guard unit > 0, amount.bitWidth <= 128 else { throw Refusal(reason: .invalidRequest) }
        if let record, record.centsUnit != String(unit) { throw Refusal(reason: .operationConflict) }
        let preview: TransferPreview
        do {
            preview = try await wallet.preview(amount)
        } catch CoinSelectionError.insufficientFunds, CoinSelectionError.emptyWallet {
            throw Refusal(reason: .insufficientBalance)
        }
        try check(binding, lease)
        guard
            preview.fullAmount == amount,
            Self.recipientAmount(preview.selectionResult, context: context) == amount
        else {
            throw Refusal(reason: .operationConflict)
        }
        return Quote(unit: unit, selection: preview.selectionResult, privacy: preview.scope == .withConfirmation)
    }

    private static func needsReview(_ value: NativeCoinageOutgoing, privacy: Bool) -> Bool {
        value.approval != .approved || (privacy && !value.privacyApproved)
    }

    private static func validate(_ intent: NativeCoinagePaymentIntent) throws {
        guard
            intent.operationId.count == 32,
            intent.peerIdentity.count == 32,
            !intent.productId.isEmpty,
            !intent.requestId.isEmpty,
            intent.amountCents > 0
        else {
            throw Refusal(reason: .invalidRequest)
        }
    }

    private func existingOutgoing(
        for intent: NativeCoinagePaymentIntent,
        immutable: NativeCoinageIntent,
        records: [NativeCoinageRecord]
    ) throws -> NativeCoinageOutgoing? {
        for case let .outgoing(existing) in records
            where existing.intent.product == intent.productId && existing.intent.request == intent.requestId {
            guard existing.intent == immutable else { throw Refusal(reason: .operationConflict) }
        }
        guard records.contains(where: { $0.operation == intent.operationId }) else { return nil }
        let record = try outgoing(records, product: intent.productId, operation: intent.operationId)
        guard record.intent == immutable else { throw Refusal(reason: .operationConflict) }
        guard record.approval != .rejected else { throw Refusal(reason: .userRejected) }
        return record
    }
}
