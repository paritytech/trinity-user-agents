import AsyncExtensions
import Foundation
import os
import SDKLogger
import StructuredConcurrency
import SubstrateSdk

/// Drives inbound top-ups to completion, restart-durably. `accept` validates and persists; the
/// `setup` subscription starts/resumes the claim task, so idempotency and recovery fall out of
/// persistence.
///
/// Secrets live only in `IncomingPaymentSecretStoring` (encrypted, wiped on settle); the record holds
/// no source and no live status. The terminal verdict is written once on settle and read back
/// exactly, so a reorg after settlement can never change what a completed top-up reports.
public final class IncomingPaymentService: IncomingPaymentServicing, @unchecked Sendable {
    private let store: any IncomingPaymentStoring
    private let secretStore: any IncomingPaymentSecretStoring
    private let sourceResolver: any IncomingPaymentSourceResolving
    private let paymentContext: IncomingPaymentContext
    private let claimCoinsService: any ClaimCoinsServicing
    private let claimAssetService: any ClaimAssetServicing
    private let verdictResolver: any CoinageGroupVerdictResolving
    private let acknowledger: any IncomingPaymentAcknowledging
    private let instanceId: CoinageInstanceId
    private let logger: SDKLoggerProtocol?
    private let acceptQueue = SerialOperationQueue()

    init(
        store: any IncomingPaymentStoring,
        secretStore: any IncomingPaymentSecretStoring,
        sourceResolver: any IncomingPaymentSourceResolving,
        paymentContext: IncomingPaymentContext,
        claimCoinsService: any ClaimCoinsServicing,
        claimAssetService: any ClaimAssetServicing,
        verdictResolver: any CoinageGroupVerdictResolving,
        acknowledger: any IncomingPaymentAcknowledging,
        instanceId: CoinageInstanceId,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.secretStore = secretStore
        self.sourceResolver = sourceResolver
        self.paymentContext = paymentContext
        self.claimCoinsService = claimCoinsService
        self.claimAssetService = claimAssetService
        self.verdictResolver = verdictResolver
        self.acknowledger = acknowledger
        self.instanceId = instanceId
        self.logger = logger
    }
}

// MARK: - IncomingPaymentServicing

public extension IncomingPaymentService {
    func accept(
        amount: Balance,
        descriptor: IncomingPaymentSourceDescriptor,
        paymentId: IncomingPaymentId,
        productId: String
    ) async throws {
        do {
            try await acceptQueue.run { [self] in
                try await performAccept(
                    amount: amount, descriptor: descriptor, paymentId: paymentId, productId: productId
                )
            }
        } catch let error as IncomingPaymentError {
            throw error
        } catch {
            throw IncomingPaymentError.unknown(reason: String(describing: error))
        }
    }

    func subscribeStatus(
        for paymentId: IncomingPaymentId,
        productId: String
    ) async throws -> AnyAsyncSequence<IncomingPaymentStatus> {
        let groupId = IncomingPayment.groupId(productId: productId, paymentId: paymentId)

        return try await paymentContext.liveStatusStream(for: groupId) { [store] in
            // Cold subscribe: a settled record returns its stored verdict exactly; an unknown one, notClaimed.
            let payment = try await store.fetch(groupId: groupId)
            guard let payment else {
                throw IncomingPaymentError.notFound(paymentId)
            }

            return payment.outcome.map(IncomingPaymentStatus.init(outcome:)) ?? .detecting
        }
    }

    func setup(with denomination: DenominationBreakdownContext) {
        Task { [weak self, paymentContext] in
            await paymentContext.setup {
                self?.runSetup(denomination: denomination)
            }
        }
    }
}

extension IncomingPaymentService {
    /// Cancels the setup subscription and every in-flight claim task.
    func throttle() {
        Task { [paymentContext] in
            await paymentContext.throttleIfNeeded()
        }
    }
}

// MARK: - Accept

private extension IncomingPaymentService {
    func performAccept(
        amount: Balance,
        descriptor: IncomingPaymentSourceDescriptor,
        paymentId: IncomingPaymentId,
        productId: String
    ) async throws {
        guard amount > 0 else {
            throw IncomingPaymentError.invalidAmount
        }

        let groupId = IncomingPayment.groupId(productId: productId, paymentId: paymentId)
        if try await store.fetch(groupId: groupId) != nil {
            throw IncomingPaymentError.alreadyExists
        }

        // Validate by resolving — a source that cannot produce signing/claim material is invalid.
        do {
            _ = try await sourceResolver.resolve(descriptor: descriptor)
        } catch {
            throw IncomingPaymentError.invalidSource(reason: String(describing: error))
        }

        try await ensureSourceFree(descriptor: descriptor)

        // Both recorded before a single transaction is built, so a resumed top-up can be picked up.
        try secretStore.save(groupId: groupId, descriptor: descriptor)

        let payment = IncomingPayment(
            paymentId: paymentId,
            productId: productId,
            amount: amount,
            createdAt: Date(),
            outcome: nil
        )
        do {
            try await store.save(payment)
        } catch {
            secretStore.remove(groupId: groupId)
            throw error
        }
    }

    /// Throws `SourceBusy` when the descriptor draws on the same funds as an active payment's. An
    /// active payment whose secret is corrupted can never claim, so it cannot hold funds busy; a
    /// store that cannot be read at all still aborts, since nothing is known about those payments.
    func ensureSourceFree(descriptor: IncomingPaymentSourceDescriptor) async throws {
        let active = try await store.fetchActivePayments()
        for other in active {
            let otherDescriptor: IncomingPaymentSourceDescriptor?
            do {
                otherDescriptor = try secretStore.fetch(groupId: other.groupId)
            } catch IncomingPaymentSecretStoreError.corrupted {
                logger?.warning("Incoming payment \(other.paymentId) secret corrupted; skipped in busy check")
                continue
            }
            guard let otherDescriptor else { continue }
            if descriptor.drawsOnSameFunds(as: otherDescriptor) {
                throw IncomingPaymentError.sourceBusy
            }
        }
    }
}

// MARK: - Setup / driving

private extension IncomingPaymentService {
    func runSetup(denomination: DenominationBreakdownContext) -> Task<Void, Never> {
        Task { [weak self] in
            guard let self else { return }
            await driveActivePayments(denomination: denomination)
        }
    }

    func driveActivePayments(denomination: DenominationBreakdownContext) async {
        do {
            for try await payments in store.observeActivePayments() {
                for payment in payments {
                    await paymentContext.process(groupId: payment.groupId) { [weak self] in
                        guard let self else { return Task {} }
                        return Task { await self.drive(payment: payment, denomination: denomination) }
                    }
                }
            }
        } catch {
            logger?.error("Incoming payments: active-payment stream failed: \(error)")
        }
    }

    func drive(payment: IncomingPayment, denomination: DenominationBreakdownContext) async {
        defer {
            Task { [paymentContext, groupId = payment.groupId] in
                await paymentContext.finish(groupId: groupId)
            }
        }

        switch lookupSecret(for: payment) {
        case .unreadable:
            return
        case .gone:
            // Can't re-run without the source, so settle from the ledger alone.
            await settleFromDurability(payment: payment, denomination: denomination)
        case let .found(descriptor):
            await runClaim(payment: payment, descriptor: descriptor, denomination: denomination)
        }
    }

    enum SecretLookup {
        case found(IncomingPaymentSourceDescriptor)
        /// Lost (Keychain wiped) or corrupted: no launch will ever read it.
        case gone
        /// Not readable right now (the Keychain before first unlock, say). Not a secret that is gone:
        /// settling it would be a verdict the ledger may still be moving towards, so the record is
        /// left for a launch that can read it.
        case unreadable
    }

    func lookupSecret(for payment: IncomingPayment) -> SecretLookup {
        do {
            guard let descriptor = try secretStore.fetch(groupId: payment.groupId) else {
                return .gone
            }
            return .found(descriptor)
        } catch IncomingPaymentSecretStoreError.corrupted {
            logger?.error("Incoming payment \(payment.paymentId) secret corrupted; settling from durability group")
            return .gone
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) secret unreadable; left for next launch: \(error)")
            return .unreadable
        }
    }

    func runClaim(
        payment: IncomingPayment,
        descriptor: IncomingPaymentSourceDescriptor,
        denomination: DenominationBreakdownContext
    ) async {
        let resolved: ResolvedIncomingSource
        do {
            resolved = try await sourceResolver.resolve(descriptor: descriptor)
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) source unresolvable; left for next launch: \(error)")
            return
        }

        var last: IncomingPaymentStatus = .detecting
        do {
            for try await detection in claimStream(for: payment, resolved: resolved, denomination: denomination) {
                last = IncomingPaymentStatus(detection: detection)
                await paymentContext.report(last, for: payment.groupId)
            }
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) claim stream failed: \(error)")
        }
        await settle(payment: payment, finalStatus: last, secretIsUsable: true)
    }

    func claimStream(
        for payment: IncomingPayment,
        resolved: ResolvedIncomingSource,
        denomination: DenominationBreakdownContext
    ) -> AnyAsyncSequence<CoinageTransferDetection> {
        let retryUntil = payment.createdAt.addingTimeInterval(CoinageConstants.topUpRetryWindow)

        switch resolved {
        case let .coins(secretKeys):
            return claimCoinsService.claim(
                coinKeys: secretKeys,
                groupId: payment.groupId,
                retryUntil: retryUntil,
                context: denomination
            )
        case let .wallet(wallet):
            return claimAssetService.claim(
                wallet: wallet,
                amount: payment.amount,
                groupId: payment.groupId,
                retryUntil: retryUntil,
                instanceId: instanceId,
                context: denomination
            )
        }
    }

    /// Writes the verdict, wipes the secret, and prompts the user on an unhappy ending. A run that
    /// ended without a verdict (window not yet closed) is left for the next launch to resume — as is
    /// one whose verdict failed to persist, so the secret stays and the user is told exactly once.
    /// - Parameter secretIsUsable: whether the source secret could actually be read. One that is gone
    ///   or corrupt is retired whatever the verdict — keeping an entry no launch will ever read helps
    ///   nobody. A readable one is retired only where the verdict says the funds moved.
    func settle(
        payment: IncomingPayment,
        finalStatus: IncomingPaymentStatus,
        secretIsUsable: Bool
    ) async {
        guard let outcome = finalStatus.terminalOutcome else {
            logger?.warning("Incoming payment \(payment.paymentId) ended without a verdict: \(finalStatus)")
            return
        }

        do {
            try await store.settle(groupId: payment.groupId, outcome: outcome)
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) failed to persist verdict: \(error)")
            return
        }
        // A readable secret is retired only by a verdict that says where the funds went. `notClaimed`
        // says they were never collected — and for a `.coins` source the stored secrets *are* the
        // money, so removing them here is what turns an uncollected top-up into an unrecoverable one.
        // Keeping them costs a Keychain entry; a retention sweep can reclaim those once one exists.
        if !secretIsUsable || outcome != .notClaimed {
            secretStore.remove(groupId: payment.groupId)
        }

        await acknowledge(payment: payment, outcome: outcome)
    }

    /// Tells the user about an unhappy verdict. A happy verdict has nothing to tell.
    func acknowledge(payment: IncomingPayment, outcome: IncomingPaymentTerminalOutcome) async {
        switch outcome {
        case .claimedPartially,
             .notClaimed:
            await acknowledger.acknowledge(
                productId: payment.productId,
                paymentId: payment.paymentId,
                requestedAmount: payment.amount,
                outcome: outcome
            )
        case .claimed:
            break
        }
    }

    /// Settles a payment whose secret is gone from its durability group alone: awaits whatever earlier
    /// runs registered to finish and values what finalized. A group that cannot be observed leaves the
    /// record for the next launch.
    func settleFromDurability(payment: IncomingPayment, denomination: DenominationBreakdownContext) async {
        do {
            let detection = try await verdictResolver.settledVerdict(
                groupId: payment.groupId,
                amount: payment.amount,
                context: denomination
            )
            let status = IncomingPaymentStatus(detection: detection)
            await paymentContext.report(status, for: payment.groupId)
            // The secret is gone or corrupt — that is why this path was taken — so it is retired with
            // the verdict whatever that verdict is.
            await settle(payment: payment, finalStatus: status, secretIsUsable: false)
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) group unobservable; left for next launch: \(error)")
        }
    }
}
