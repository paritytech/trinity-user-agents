import Foundation
import Products
import SubstrateSdk

/// Carries the state of an in-flight contact pick. Bridges the picker UI to
/// the async caller through a `CheckedContinuation`, delivered exactly once.
/// A dismissal delivers `nil`, which the core reports to the product as a
/// dismissal rather than a failure.
///
/// It carries no contact list: the picker reads the same store the chat search
/// does, so there is one place that decides who is offered.
@MainActor
final class ContactPickContext {
    nonisolated let productId: ProductId

    private var continuation: CheckedContinuation<AccountId?, Never>?

    init(productId: ProductId) {
        self.productId = productId
    }

    deinit {
        continuation?.resume(returning: nil)
    }

    func setContinuation(_ continuation: CheckedContinuation<AccountId?, Never>) {
        self.continuation = continuation
    }

    func deliver(_ picked: AccountId?) {
        continuation?.resume(returning: picked)
        continuation = nil
    }
}
