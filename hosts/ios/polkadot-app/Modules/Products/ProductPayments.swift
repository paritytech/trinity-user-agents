import Foundation
import AsyncExtensions
import Coinage
import Products
import SubstrateSdk

/// The payment steps both product bridges run: the JS-bridge handlers in
/// `ProductsNativeApi+Payment` and the Rust core's ``RustPaymentsBridge``. It speaks the engines'
/// own types; each caller maps their errors and statuses to its wire.
struct ProductPayments {
    let support: PaymentsSupport
    let approvalRequester: PaymentApprovalRequesting
    let privacyConfirmer: PaymentPrivacyConfirming
    let recyclingStrategy: any CoinageRecyclingStrategyProviding

    // MARK: - Balance

    func balanceStream() async throws -> AnyAsyncSequence<CoinageBalance> {
        try await support.coinageService.coinageBalanceService().balanceStream
    }

    /// Whether what is spendable on-chain right now (private plus gaining-privacy funds; minting
    /// funds cannot be waited for) covers `amount`.
    func canSpend(_ amount: Balance) async throws -> Bool {
        var balance = CoinageBalance.empty
        for try await value in try await balanceStream().prefix(1) {
            balance = value
        }

        return balance.spendableByPayment >= amount
    }

    // MARK: - Payment

    /// Shows the payment request sheet (auto-approved for allowlisted products), then warns whenever
    /// private vouchers alone cannot pay — a voucher still gaining privacy or a coin loaded just to
    /// be unloaded gives up privacy — unless the preset is `minPrivacy`. The privacy warning is never
    /// allowlisted. `false` when the user declined either.
    func awaitUserConsent(productId: String, amount: Balance, destination: AccountId) async throws -> Bool {
        let decision = await approvalRequester.requestApproval(
            productId: productId,
            amount: amount,
            destination: destination
        )
        guard decision == .approved else { return false }

        guard recyclingStrategy.strategy != .minPrivacy else { return true }
        guard try await !support.coinageService.canExecuteExternalPaymentPrivately(amount: amount) else {
            return true
        }

        return await privacyConfirmer.confirmGainingPrivacySpend(amount: amount)
    }

    /// Registers the payment. Throws `ExternalPaymentError`.
    func initiatePayment(
        productId: String,
        id: PaymentRequestId,
        amount: Balance,
        destination: AccountId
    ) async throws {
        try await support.coinageService.initiateExternalPayment(
            productId: productId,
            paymentId: Self.paymentKey(id),
            amountInPlanks: amount,
            destination: destination
        )
    }

    /// Ends after the first terminal status; fails with `ExternalPaymentError.notFound` for an
    /// unknown id.
    func paymentStatuses(productId: String, id: PaymentRequestId) -> AnyAsyncSequence<ExternalPaymentStatus> {
        support.coinageService.subscribeExternalPaymentStatus(
            productId: productId,
            paymentId: Self.paymentKey(id)
        )
    }

    // MARK: - Top-up

    /// Registers an idempotent top-up bound to `(productId, id)` and returns once initialization has
    /// concluded; `IncomingPaymentService` drives the claim. Throws ``ProductTopUpSourceError`` when
    /// the source cannot be described, `IncomingPaymentError` otherwise.
    func acceptTopUp(productId: String, id: PaymentTopUpId, amount: Balance, source: PaymentTopUpSource) async throws {
        let descriptor: IncomingPaymentSourceDescriptor
        do {
            descriptor = try Self.incomingPaymentDescriptor(from: source, productId: productId)
        } catch {
            throw ProductTopUpSourceError(underlying: error)
        }

        try await support.incomingPaymentService.accept(
            amount: amount,
            descriptor: descriptor,
            paymentId: Self.topUpKey(id),
            productId: productId
        )
    }

    /// The live statuses of an active top-up, or the stored verdict of a settled one. Throws
    /// `IncomingPaymentError.notFound` for an unknown id.
    func topUpStatuses(productId: String, id: PaymentTopUpId) async throws -> AnyAsyncSequence<IncomingPaymentStatus> {
        try await support.incomingPaymentService.subscribeStatus(for: Self.topUpKey(id), productId: productId)
    }
}

/// A top-up source that could not be described as persisted bytes.
struct ProductTopUpSourceError: Error {
    let underlying: Error
}

extension CoinageBalance {
    /// What a product payment can spend right now: private plus gaining-privacy funds.
    var spendableByPayment: Balance {
        availablePrivate + gainingPrivacy.amount
    }
}

// MARK: - Engine keys

private extension ProductPayments {
    /// The engines key records by these strings, so both bridges must spell an id the same way.
    static func paymentKey(_ id: PaymentRequestId) -> String {
        id.toHex(includePrefix: true)
    }

    static func topUpKey(_ id: PaymentTopUpId) -> String {
        id.toHex()
    }

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
