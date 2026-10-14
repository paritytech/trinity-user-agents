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

        let coinageService = try requirePaymentsSupport().coinageService
        let balanceService = try await coinageService.coinageBalanceService()
        return balanceService.balanceStream
            .map { balance in
                PaymentBalance(available: balance.total)
            }
            .eraseToAnyAsyncSequence()
    }

    func requestPayment(amount: Balance, destination: AccountId, id: PaymentRequestId) async throws {
        let coinageService = try requirePaymentsSupport().coinageService

        try await checkSufficientBalance(amount: amount)
        try await awaitUserApproval(amount: amount, destination: destination)
        try await awaitPrivacyConsentIfNeeded(amount: amount)

        do {
            try await coinageService.initiateExternalPayment(
                productId: productId,
                paymentId: id.toHex(includePrefix: true),
                amountInPlanks: amount,
                destination: destination
            )
        } catch ExternalPaymentError.alreadyExists {
            throw HostPaymentRequestError.alreadyExists
        }
    }

    func subscribePaymentStatus(id: PaymentRequestId) async throws -> AnyAsyncSequence<HostPaymentStatus> {
        let coinageService = try requirePaymentsSupport().coinageService
        let statuses = coinageService.subscribeExternalPaymentStatus(
            productId: productId,
            paymentId: id.toHex(includePrefix: true)
        )

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
        let incomingPaymentService = try requirePaymentsSupport().incomingPaymentService

        let descriptor: IncomingPaymentSourceDescriptor
        do {
            descriptor = try Self.incomingPaymentDescriptor(from: source, productId: productId)
        } catch {
            logger.error("Top-up source could not be described: \(error)")
            throw HostPaymentTopUpError.invalidSource
        }

        do {
            try await incomingPaymentService.accept(
                amount: amount,
                descriptor: descriptor,
                paymentId: id.toHex(),
                productId: productId
            )
        } catch {
            logger.error("Top-up could not be registered: \(error)")
            throw HostPaymentTopUpError(error, unknownReason: Self.topUpRegistrationFailed)
        }
    }

    func subscribePaymentTopUpStatus(
        id: PaymentTopUpId
    ) async throws -> AnyAsyncSequence<HostPaymentTopUpStatus> {
        do {
            let incomingPaymentService = try requirePaymentsSupport().incomingPaymentService

            return try await incomingPaymentService.subscribeStatus(
                for: id.toHex(),
                productId: productId
            )
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

    /// Checks the amount against what is spendable on-chain right now (private plus gaining-privacy
    /// funds; minting funds cannot be waited for). The permission is only read, never prompted: with
    /// `balanceAccess` the product already knows balances and gets `insufficientBalance`; without it
    /// the shortfall is reported as `rejected` so nothing leaks.
    func checkSufficientBalance(amount: Balance) async throws {
        let coinageService = try requirePaymentsSupport().coinageService
        let balanceService = try await coinageService.coinageBalanceService()

        var balance = CoinageBalance.empty
        for try await value in balanceService.balanceStream.prefix(1) {
            balance = value
        }

        guard balance.availablePrivate + balance.gainingPrivacy.amount < amount else { return }

        let knowsBalance = try await permissionGuard.check(productId: productId, permission: .balanceAccess)
        throw knowsBalance ? HostPaymentRequestError.insufficientBalance : HostPaymentRequestError.rejected
    }

    /// Warns whenever private vouchers alone cannot pay — a voucher still gaining privacy or a coin
    /// loaded just to be unloaded gives up privacy — unless the preset is `minPrivacy`. Not allowlisted.
    func awaitPrivacyConsentIfNeeded(amount: Balance) async throws {
        guard recyclingStrategy.strategy != .minPrivacy else { return }

        let coinageService = try requirePaymentsSupport().coinageService
        guard try await !coinageService.canExecuteExternalPaymentPrivately(amount: amount) else { return }

        guard await paymentPrivacyConfirmer.confirmGainingPrivacySpend(amount: amount) else {
            throw HostPaymentRequestError.rejected
        }
    }

    /// Auto-approved for allowlisted products; everyone else sees the payment request sheet.
    func awaitUserApproval(amount: Balance, destination: AccountId) async throws {
        let decision = await paymentApprovalRequester.requestApproval(
            productId: productId,
            amount: amount,
            destination: destination
        )

        guard decision == .approved else {
            throw HostPaymentRequestError.rejected
        }
    }
}

// MARK: - Top-Up Source Description

private extension ProductsNativeApi {
    /// Describes the product-facing source as the persisted bytes the claim is later resolved from —
    /// the full derivation **path** for a product account (never a derived key), the raw key otherwise.
    /// Resolution + validation happen later, in `IncomingPaymentSourceResolver`.
    static func incomingPaymentDescriptor(
        from source: PaymentTopUpSource,
        productId: String
    ) throws -> IncomingPaymentSourceDescriptor {
        switch source {
        case let .productAccount(derivationIndex):
            let derivationPath = try ProductAccountId(
                productId: productId,
                derivationIndex: derivationIndex
            ).derivationPath()

            return .productAccount(derivationPath: derivationPath)
        case let .privateKey(secretKey):
            return .privateKey(secretKey: secretKey)
        case let .coins(secretKeys):
            return .coins(secretKeys: secretKeys)
        }
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
