import AsyncExtensions
import Foundation
import os
@testable import Coinage

/// In-memory `ExternalPaymentStoring`. Every save republishes the whole table so the observe
/// streams behave like `subscribeSnapshot` / `subscribeSingle`: current value first, then updates.
final class InMemoryExternalPaymentStore: ExternalPaymentStoring, @unchecked Sendable {
    struct Failure: Error {}

    private let payments = OSAllocatedUnfairLock(initialState: [String: ExternalPayment]())
    private let subject = AsyncCurrentValueSubject<[ExternalPayment]>([])

    var saveError: Error?

    init(seed: [ExternalPayment] = []) {
        payments.withLock { table in
            for payment in seed {
                table[payment.identifier] = payment
            }
        }
        publish()
    }

    func save(payment: ExternalPayment) async throws {
        if let saveError { throw saveError }
        payments.withLock { $0[payment.identifier] = payment }
        publish()
    }

    func fetchPayment(byId id: String) async throws -> ExternalPayment? {
        payments.withLock { $0[id] }
    }

    func observePayment(id: String) -> AnyAsyncSequence<ExternalPayment?> {
        subject
            .map { $0.first { $0.identifier == id } }
            .eraseToAnyAsyncSequence()
    }

    func observeNonTerminalPayments() -> AnyAsyncSequence<[ExternalPayment]> {
        subject
            .map { $0.filter { !$0.stage.isTerminal } }
            .eraseToAnyAsyncSequence()
    }

    // MARK: - Test inspection

    struct Timeout: Error {}

    /// Resolves with the first snapshot of `id` matching `predicate`: event-driven, so a slow runner
    /// only delays the test. `timeout` exists to fail instead of hang.
    func awaitPayment(
        id: String,
        timeout: Duration = .seconds(100),
        where predicate: @escaping @Sendable (ExternalPayment) -> Bool
    ) async throws -> ExternalPayment {
        try await race(timeout: timeout) { [self] in
            for try await payment in observePayment(id: id) {
                if let payment, predicate(payment) { return payment }
            }
            throw Timeout()
        }
    }

    /// Resolves with the first whole-table snapshot matching `predicate`.
    func awaitPayments(
        timeout: Duration = .seconds(100),
        where predicate: @escaping @Sendable ([ExternalPayment]) -> Bool
    ) async throws -> [ExternalPayment] {
        try await race(timeout: timeout) { [self] in
            for try await payments in subject.eraseToAnyAsyncSequence() where predicate(payments) {
                return payments
            }
            throw Timeout()
        }
    }

    func payment(id: String) -> ExternalPayment? {
        payments.withLock { $0[id] }
    }

    func all() -> [ExternalPayment] {
        payments.withLock { Array($0.values) }
    }

    private func race<T: Sendable>(
        timeout: Duration,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            group.addTask { try await operation() }
            group.addTask {
                try await Task.sleep(for: timeout)
                throw Timeout()
            }
            guard let first = try await group.next() else { throw Timeout() }
            group.cancelAll()
            return first
        }
    }

    private func publish() {
        subject.send(all().sorted { $0.createdAt < $1.createdAt })
    }
}
