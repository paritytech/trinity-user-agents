import Foundation
import StateMachine

/// Wraps ``ExternalPaymentStoring`` for a specific payment ID,
/// conforming to ``StateMachineStoring`` for use with the generic ``StateMachine``.
final class ExternalPaymentStateStore: StateMachineStoring {
    typealias PersistentValue = ExternalPayment

    let paymentId: String
    let store: ExternalPaymentStoring

    init(paymentId: String, store: ExternalPaymentStoring) {
        self.paymentId = paymentId
        self.store = store
    }

    func loadStateMemo() async throws -> ExternalPayment {
        guard let payment = try await store.fetchPayment(byId: paymentId) else {
            throw ExternalPaymentStateStoreError.paymentNotFound(paymentId)
        }
        return payment
    }

    func saveStateMemo(_ value: ExternalPayment) async throws {
        try await store.save(payment: value)
    }
}

enum ExternalPaymentStateStoreError: Error {
    case paymentNotFound(String)
}
