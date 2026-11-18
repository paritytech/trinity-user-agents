import struct TrUAPIHost.ProductId
import Foundation
import AsyncExtensions
import Coinage
import KeyDerivation
import Products
import SubstrateSdk

/// The payment steps the legacy product host API runs, from the JS-bridge handlers in
/// `ProductsNativeApi+Payment`. It speaks the wallet's own types; the caller maps errors and
/// statuses to its wire.
struct ProductPayments {
    let support: PaymentsSupport
    let approvalRequester: PaymentApprovalRequesting
    let privacyConfirmer: PaymentPrivacyConfirming
    let recyclingStrategy: any CoinageRecyclingStrategyProviding

    // MARK: - Balance

    func balanceStream() -> AnyAsyncSequence<CoinageBalance> {
        support.coinageService.balanceStream()
    }

    /// Whether what is spendable on-chain right now (private plus gaining-privacy funds; minting
    /// funds cannot be waited for) covers `amount`.
    func canSpend(_ amount: Balance) async throws -> Bool {
        var balance = CoinageBalance.empty
        for try await value in balanceStream().prefix(1) {
            balance = value
        }

        return balance.spendableByPayment >= amount
    }

    // MARK: - Payment

    /// Shows the payment request sheet (auto-approved for allowlisted products), then, unless the
    /// preset is `minPrivacy`, warns whenever private value alone cannot pay: spending value still
    /// gaining privacy gives up privacy. The privacy warning is never allowlisted. `nil` when the user
    /// declined either; otherwise whether the payment may spend value still gaining privacy.
    func awaitUserConsent(productId: ProductId, amount: Balance, destination: AccountId) async throws -> Bool? {
        let decision = await approvalRequester.requestApproval(
            productId: productId,
            amount: amount,
            destination: destination
        )
        guard decision == .approved else { return nil }

        guard try await support.coinageService.previewExternalPayment(for: amount) != .privately else {
            return false
        }
        guard recyclingStrategy.strategy != .minPrivacy else { return true }

        return await privacyConfirmer.confirmGainingPrivacySpend(amount: amount) ? true : nil
    }

    /// Registers the payment. Throws `ExternalPaymentError`. `confirmed` is the consent from
    /// ``awaitUserConsent(productId:amount:destination:)``.
    func initiatePayment(
        productId: ProductId,
        id: PaymentRequestId,
        amount: Balance,
        destination: AccountId,
        confirmed: Bool
    ) async throws {
        try await support.coinageService.initiateExternalPayment(
            ExternalPaymentRequest(
                productId: productId,
                paymentId: Self.paymentKey(id),
                amountInPlanks: amount,
                destination: destination,
                confirmed: confirmed
            )
        )
    }

    /// Ends after the first terminal status; fails with `ExternalPaymentError.notFound` for an
    /// unknown id.
    func paymentStatuses(productId: ProductId, id: PaymentRequestId) -> AnyAsyncSequence<ExternalPaymentStatus> {
        support.coinageService.subscribeExternalPaymentStatus(
            productId: productId,
            paymentId: Self.paymentKey(id)
        )
    }

    // MARK: - Top-up

    /// Starts a top-up bound to `(productId, id)`; the wallet drives it from there. Throws
    /// ``ProductTopUpSourceError`` when the source cannot be resolved, `IncomingPaymentError` otherwise.
    func acceptTopUp(
        productId: ProductId,
        id: PaymentTopUpId,
        amount: Balance,
        source: PaymentTopUpSource
    ) async throws {
        let topUpSource: CoinageTopUpRequest.Source
        do {
            topUpSource = try Self.topUpSource(from: source, productId: productId)
        } catch {
            throw ProductTopUpSourceError(underlying: error)
        }

        try await support.coinageService.acceptTopUp(
            CoinageTopUpRequest(
                productId: productId,
                paymentId: Self.topUpKey(id),
                amount: amount,
                source: topUpSource
            )
        )
    }

    /// The statuses of a top-up until it ends. Fails with `IncomingPaymentError.notFound` for an
    /// unknown id.
    func topUpStatuses(productId: ProductId, id: PaymentTopUpId) -> AnyAsyncSequence<IncomingPaymentStatus> {
        support.coinageService.subscribeTopUpStatus(productId: productId, paymentId: Self.topUpKey(id))
    }
}

/// A top-up source that could not be resolved.
struct ProductTopUpSourceError: Error {
    let underlying: Error
}

extension CoinageBalance {
    /// What a product payment can spend right now: private plus gaining-privacy funds.
    var spendableByPayment: Balance {
        availablePrivate + gainingPrivacy.amount
    }
}

// MARK: - Operation keys

private extension ProductPayments {
    /// The wallet keys operations by these strings. Payments are spelled with the `0x` prefix and
    /// top-ups without.
    static func paymentKey(_ id: PaymentRequestId) -> String {
        id.toHex(includePrefix: true)
    }

    static func topUpKey(_ id: PaymentTopUpId) -> String {
        id.toHex()
    }

    /// The account that pays a top-up, or the coins it claims: a product account is derived from the
    /// seed along its full derivation path, a raw key is used as given.
    static func topUpSource(
        from source: PaymentTopUpSource,
        productId: ProductId
    ) throws -> CoinageTopUpRequest.Source {
        switch source {
        case let .productAccount(derivationIndex):
            let derivationPath = try ProductAccountId(
                productId: productId,
                derivationIndex: derivationIndex
            ).derivationPath()

            return .funding(
                DynamicDerivedWallet(derivationPath: derivationPath, entropyManager: RootEntropyManager.shared)
            )
        case let .privateKey(secretKey):
            return .funding(DynamicDerivedWallet(secretKeyProvider: { secretKey }))
        case let .coins(secretKeys):
            return .coins(secretKeys: secretKeys)
        }
    }
}
