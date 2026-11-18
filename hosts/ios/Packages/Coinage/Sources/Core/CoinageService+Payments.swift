import struct TrUAPIHost.ProductId
import AsyncExtensions
import BigInt
import Foundation
import KeyDerivation
import SubstrateSdk
import TrUAPIHost

extension CoinageService {
    public func loadVouchers(amount: BigUInt, externalAssetHolder: any WalletManaging) async throws {
        let units = try await core.units()
        let funding = try WalletFunding(wallet: externalAssetHolder)
        let id = try CoinageOperationId.random()

        let progress = followOperation(id) { coinage in
            try await coinage.topUp(id: id.value, retry: .never, amount: units.cents(ofPlanks: amount), funding: funding)
        }
        for try await _ in progress {}
    }

    public func acceptTopUp(_ request: CoinageTopUpRequest) async throws {
        guard request.amount > 0 else {
            throw IncomingPaymentError.invalidAmount
        }

        let units = try await core.units()
        let id = try Self.topUpId(productId: request.productId, paymentId: request.paymentId)
        let retry = Retry.until(Date().addingTimeInterval(CoinageConstants.topUpRetryWindow).unixSeconds)
        let cents = units.cents(ofPlanks: request.amount)

        let started: Started
        switch request.source {
        case let .funding(wallet):
            let funding: WalletFunding
            do {
                funding = try WalletFunding(wallet: wallet)
            } catch {
                throw IncomingPaymentError.invalidSource(reason: "the source account has no usable key")
            }
            started = try await core.use { try await $0.topUp(id: id.value, retry: retry, amount: cents, funding: funding) }
        case let .coins(secretKeys):
            guard !secretKeys.isEmpty else {
                throw IncomingPaymentError.invalidSource(reason: "no coins given")
            }
            started = try await core.use { try await $0.import(id: id.value, retry: retry, coins: secretKeys) }
        }

        guard started == .new else {
            throw IncomingPaymentError.alreadyExists
        }

        acknowledgeWhenSettled(request, id: id, units: units)
    }

    public func subscribeTopUpStatus(
        productId: ProductId,
        paymentId: IncomingPaymentId
    ) -> AnyAsyncSequence<IncomingPaymentStatus> {
        let core = core

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let id = try Self.topUpId(productId: productId, paymentId: paymentId)
                    let units = try await core.units()
                    for try await item in core.statuses(of: id) {
                        continuation.yield(item.incomingPaymentStatus(units: units))
                        if item.status.isTerminal { break }
                    }
                    continuation.finish()
                } catch CoinageError.UnknownOperation {
                    continuation.finish(throwing: IncomingPaymentError.notFound(paymentId))
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    public func previewExternalPayment(for amount: BigUInt) async throws -> ExternalPaymentPreview {
        let current = try await balance()

        if amount <= current.availablePrivate {
            return .privately
        }
        if amount <= current.available {
            return .spendsGainingPrivacy
        }
        return .notEnoughBalance(available: current.available)
    }

    public func initiateExternalPayment(_ request: ExternalPaymentRequest) async throws {
        guard !request.paymentId.isEmpty else {
            throw ExternalPaymentError.invalidPaymentId
        }

        let units = try await core.units()
        let id = try Self.externalPaymentId(productId: request.productId, paymentId: request.paymentId)
        let retry = Retry.until(Date().addingTimeInterval(CoinageConstants.externalPaymentRetryWindow).unixSeconds)

        let started = try await core.use { coinage in
            try await coinage.offload(
                id: id.value,
                retry: retry,
                amount: units.cents(ofPlanks: request.amountInPlanks),
                to: request.destination,
                confirmed: request.confirmed
            )
        }

        guard started == .new else {
            throw ExternalPaymentError.alreadyExists
        }
    }

    public func subscribeExternalPaymentStatus(
        productId: ProductId?,
        paymentId: String
    ) -> AnyAsyncSequence<ExternalPaymentStatus> {
        let core = core

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let id = try Self.externalPaymentId(productId: productId, paymentId: paymentId)
                    let units = try await core.units()
                    for try await item in core.statuses(of: id) {
                        let status = item.externalPaymentStatus(units: units)
                        continuation.yield(status)
                        if status != .processing { break }
                    }
                    continuation.finish()
                } catch CoinageError.UnknownOperation {
                    continuation.finish(throwing: ExternalPaymentError.notFound)
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }
}

private extension CoinageService {
    static func topUpId(productId: ProductId, paymentId: IncomingPaymentId) throws -> CoinageOperationId {
        try .of("top up:\(productId.rawValue):\(paymentId)")
    }

    static func externalPaymentId(productId: ProductId?, paymentId: String) throws -> CoinageOperationId {
        try .of("external:\(productId?.rawValue ?? "native"):\(paymentId)")
    }

    /// Tells the user when a product's top-up ended short, long after the product's call returned.
    func acknowledgeWhenSettled(_ request: CoinageTopUpRequest, id: CoinageOperationId, units: CoinageUnits) {
        guard let topUpAcknowledger else { return }

        Task { [core, logger] in
            do {
                var last: IncomingPaymentStatus?
                for try await item in core.statuses(of: id) {
                    last = item.incomingPaymentStatus(units: units)
                    if item.status.isTerminal { break }
                }
                guard let outcome = last?.terminalOutcome else { return }

                await topUpAcknowledger.acknowledge(
                    productId: request.productId,
                    paymentId: request.paymentId,
                    requestedAmount: request.amount,
                    outcome: outcome
                )
            } catch {
                logger?.error("Top-up \(request.paymentId) could not be followed: \(error)")
            }
        }
    }
}

/// Pays a top-up from `wallet`'s account, signing every payload the core asks for.
private final class WalletFunding: NativeFunding, @unchecked Sendable {
    private let wallet: any WalletManaging
    private let publicKey: Data

    init(wallet: any WalletManaging) throws {
        self.wallet = wallet
        publicKey = try wallet.getRawPublicKey()
    }

    func account() -> Bytes32 {
        publicKey
    }

    func sign(payload: Data) -> Bytes64? {
        guard case let .sr25519(signature) = try? wallet.sign(data: payload) else {
            return nil
        }
        return signature
    }
}
