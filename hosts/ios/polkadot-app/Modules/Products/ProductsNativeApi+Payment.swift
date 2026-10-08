import BigInt
import Foundation
import Coinage
import KeyDerivation
import Products
import SubstrateSdk
import AsyncExtensions
import StructuredConcurrency

// MARK: - Payments

extension ProductsNativeApi {
    func subscribePaymentBalance() async throws -> AnyAsyncSequence<PaymentBalance> {
        guard
            try await permissionGuard.consumePermission(
                productId: productId,
                permission: .balanceAccess
            ) else {
            throw ProductNativeApiError.permissionDenied
        }

        return try await requirePayments().balanceStream()
            .map { balance in
                PaymentBalance(available: balance.total)
            }
            .eraseToAnyAsyncSequence()
    }

    func requestPayment(amount: Balance, destination: AccountId, id: PaymentRequestId) async throws {
        let payments = try requirePayments()

        try await checkSufficientBalance(amount: amount, payments: payments)
        guard try await payments.awaitUserConsent(productId: productId, amount: amount, destination: destination) else {
            throw HostPaymentRequestError.rejected
        }

        do {
            try await payments.initiatePayment(productId: productId, id: id, amount: amount, destination: destination)
        } catch ExternalPaymentError.alreadyExists {
            throw HostPaymentRequestError.alreadyExists
        }
    }

    func subscribePaymentStatus(id: PaymentRequestId) async throws -> AnyAsyncSequence<HostPaymentStatus> {
        let statuses = try requirePayments().paymentStatuses(productId: productId, id: id)

        return AsyncThrowingStream<HostPaymentStatus, Error> { continuation in
            let task = Task {
                do {
                    for try await status in statuses {
                        continuation.yield(HostPaymentStatus(status: status))
                    }
                    continuation.finish()
                } catch ExternalPaymentError.notFound {
                    continuation.finish(throwing: HostPaymentStatusError.notFound)
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    /// Registers an idempotent top-up bound to `(productId, id)` and returns once initialization has
    /// concluded. The claim is driven by `IncomingPaymentService`; the product observes progress via
    /// ``subscribePaymentTopUpStatus(id:)``.
    func paymentTopUp(amount: Balance, source: PaymentTopUpSource, id: PaymentTopUpId) async throws {
        let payments = try requirePayments()

        do {
            try await payments.acceptTopUp(productId: productId, id: id, amount: amount, source: source)
        } catch let error as ProductTopUpSourceError {
            logger.error("Top-up source could not be described: \(error.underlying)")
            throw HostPaymentTopUpError.invalidSource
        } catch {
            logger.error("Top-up could not be registered: \(error)")
            throw HostPaymentTopUpError(error, unknownReason: Self.topUpRegistrationFailed)
        }
    }

    func subscribePaymentTopUpStatus(
        id: PaymentTopUpId
    ) async throws -> AnyAsyncSequence<HostPaymentTopUpStatus> {
        do {
            return try await requirePayments().topUpStatuses(productId: productId, id: id)
                .map { HostPaymentTopUpStatus(status: $0) }
                .eraseToAnyAsyncSequence()
        } catch {
            logger.error("Top-up status could not be observed: \(error)")
            throw HostPaymentTopUpError(error, unknownReason: Self.topUpStatusUnavailable)
        }
    }
}

// MARK: - Wire reasons

private extension ProductsNativeApi {
    /// What a third-party product is told on an unclassified failure. The real error is logged; a
    /// CoreData or Keychain dump is not for product scripts.
    static let topUpRegistrationFailed = "top-up could not be registered"
    static let topUpStatusUnavailable = "top-up status is unavailable"
}

// MARK: - Payment Request Checks

private extension ProductsNativeApi {
    func requirePaymentsSupport() throws -> PaymentsSupport {
        guard let paymentsSupport else {
            logger.error("Payment feature requested but payments support is unavailable")
            throw ProductNativeApiError.paymentsNotSupported
        }

        return paymentsSupport
    }

    func requirePayments() throws -> ProductPayments {
        try ProductPayments(
            support: requirePaymentsSupport(),
            approvalRequester: paymentApprovalRequester,
            privacyConfirmer: paymentPrivacyConfirmer,
            recyclingStrategy: recyclingStrategy
        )
    }

    /// Checks the amount against what is spendable on-chain right now (private plus gaining-privacy
    /// funds; minting funds cannot be waited for). The permission is only read, never prompted: with
    /// `balanceAccess` the product already knows balances and gets `insufficientBalance`; without it
    /// the shortfall is reported as `rejected` so nothing leaks.
    func checkSufficientBalance(amount: Balance, payments: ProductPayments) async throws {
        guard try await !payments.canSpend(amount) else { return }

        let knowsBalance = try await permissionGuard.check(productId: productId, permission: .balanceAccess)
        throw knowsBalance ? HostPaymentRequestError.insufficientBalance : HostPaymentRequestError.rejected
    }
}

// MARK: - Wire Mapping

private extension HostPaymentTopUpError {
    /// The coded error for `error`; anything that is not a classified `IncomingPaymentError` becomes
    /// `unknown` with the generic `unknownReason` rather than the error's own description.
    init(_ error: any Error, unknownReason: String) {
        switch error as? IncomingPaymentError {
        case .alreadyExists: self = .alreadyExists
        case .invalidSource: self = .invalidSource
        case .sourceBusy: self = .sourceBusy
        case .invalidAmount: self = .unknown(reason: "amount must be positive")
        case let .notFound(paymentId): self = .notFound(paymentId)
        case .unknown,
             .none: self = .unknown(reason: unknownReason)
        }
    }
}

private extension HostPaymentStatus {
    init(status: ExternalPaymentStatus) {
        switch status {
        case .processing: self = .processing
        case .completed: self = .completed
        case let .partiallyCompleted(settled): self = .partiallyClaimed(settledInPlanks: settled)
        case let .failed(reason): self = .failed(reason: reason)
        }
    }
}

private extension HostPaymentTopUpStatus {
    init(status: IncomingPaymentStatus) {
        switch status {
        case .detecting: self = .detecting
        case .claiming: self = .claiming
        case let .claimed(finalized): self = .claimed(finalized: finalized)
        case let .claimedPartially(actualClaimed): self = .claimedPartially(actualClaimed: actualClaimed)
        case .notClaimed: self = .notClaimed
        }
    }
}
