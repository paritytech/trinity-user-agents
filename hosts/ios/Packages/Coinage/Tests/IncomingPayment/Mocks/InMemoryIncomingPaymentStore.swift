import AsyncExtensions
import Foundation
import os
@testable import Coinage

/// In-memory `IncomingPaymentStoring` for tests. Records hold no secret material; `settle` writes the
/// terminal verdict. `saveError`/`fetchError` inject failures.
final class InMemoryIncomingPaymentStore: IncomingPaymentStoring, @unchecked Sendable {
    struct Failure: Error {}

    private struct State {
        var payments: [CoinageTxGroupId: IncomingPayment] = [:]
        var settledGroupIds: [CoinageTxGroupId] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let activeSubject = AsyncCurrentValueSubject<[IncomingPayment]>([])
    private let beforeSave: (@Sendable () async -> Void)?

    var saveError: Error?
    var fetchError: Error?
    var settleError: Error?

    init(
        seed: [IncomingPayment] = [],
        beforeSave: (@Sendable () async -> Void)? = nil
    ) {
        self.beforeSave = beforeSave
        state.withLock { state in
            for payment in seed {
                state.payments[payment.groupId] = payment
            }
        }
        publishActive()
    }

    func save(
        _ payment: IncomingPayment,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws {
        await beforeSave?()
        if let saveError { throw saveError }
        try state.withLock {
            try authorization()
            guard $0.payments[payment.groupId] == nil else { throw IncomingPaymentError.alreadyExists }
            $0.payments[payment.groupId] = payment
        }
        publishActive()
    }

    func fetch(groupId: CoinageTxGroupId) async throws -> IncomingPayment? {
        if let fetchError { throw fetchError }
        return state.withLock { $0.payments[groupId] }
    }

    func fetchActivePayments() async throws -> [IncomingPayment] {
        state.withLock { Array($0.payments.values.filter { $0.outcome == nil }) }
    }

    func settle(
        groupId: CoinageTxGroupId,
        ownerId: Data,
        outcome: IncomingPaymentTerminalOutcome,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws {
        if let settleError { throw settleError }
        try state.withLock { state in
            try authorization()
            guard let existing = state.payments[groupId], existing.ownerId == ownerId else { throw Failure() }
            state.settledGroupIds.append(groupId)
            state.payments[groupId] = IncomingPayment(
                paymentId: existing.paymentId,
                productId: existing.productId,
                amount: existing.amount,
                createdAt: existing.createdAt,
                outcome: outcome,
                ownerId: existing.ownerId
            )
        }
        publishActive()
    }

    func observeActivePayments() -> AnyAsyncSequence<[IncomingPayment]> {
        activeSubject.eraseToAnyAsyncSequence()
    }

    // MARK: - Test inspection

    func settledGroupIds() -> [CoinageTxGroupId] {
        state.withLock { $0.settledGroupIds }
    }

    func payment(for groupId: CoinageTxGroupId) -> IncomingPayment? {
        state.withLock { $0.payments[groupId] }
    }

    private func publishActive() {
        let active = state.withLock { Array($0.payments.values.filter { $0.outcome == nil }) }
        activeSubject.send(active)
    }
}
