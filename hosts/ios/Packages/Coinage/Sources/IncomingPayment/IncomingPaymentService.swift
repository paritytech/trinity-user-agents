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
    private let lifecycle: CoinageLifecycle?
    private let ownerId: Data
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
        logger: SDKLoggerProtocol?,
        lifecycle: CoinageLifecycle? = nil,
        ownerId: Data
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
        self.lifecycle = lifecycle
        self.ownerId = ownerId
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
            let operation = try lifecycle?.captureOperation()
            try await CoinageLifecycle.$operation.withValue(operation) {
                try await acceptQueue.run { [self] in
                    try await performAccept(
                        amount: amount, descriptor: descriptor, paymentId: paymentId, productId: productId,
                        operation: operation
                    )
                }
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

        let operation = try lifecycle?.captureOperation()
        guard let payment = try await store.fetch(groupId: groupId), payment.ownerId == ownerId else {
            throw IncomingPaymentError.notFound(paymentId)
        }
        if let lifecycle, let operation { try lifecycle.check(operation) }
        return try await paymentContext.liveStatusStream(for: groupId) {
            return payment.outcome.map(IncomingPaymentStatus.init(outcome:)) ?? .detecting
        }
    }

    func setup(with denomination: DenominationBreakdownContext) {
        do {
            let operation = try lifecycle?.captureOperation()
            Task { [weak self, paymentContext] in
                do {
                    try await CoinageLifecycle.$operation.withValue(operation) {
                        try await paymentContext.setup {
                            guard let self else { return nil }
                            if let lifecycle = self.lifecycle, let operation {
                                return try lifecycle.withEffect(operation) {
                                    self.runSetup(denomination: denomination)
                                }
                            }
                            return self.runSetup(denomination: denomination)
                        }
                    }
                } catch {
                    self?.logger?.warning("Incoming payment setup skipped: \(error)")
                }
            }
        } catch {
            logger?.warning("Incoming payment setup skipped: \(error)")
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
        productId: String,
        operation: CoinageLifecycle.Operation?
    ) async throws {
        if amount == 0 {
            guard case .coins = descriptor else {
                throw IncomingPaymentError.invalidAmount
            }
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

        let authorization: @Sendable () throws -> Void = { [lifecycle] in
            if let lifecycle, let operation { try lifecycle.check(operation) }
        }
        // Both recorded before a single transaction is built, so a resumed top-up can be picked up.
        if let lifecycle, let operation {
            try lifecycle.withEffect(operation) {
                try secretStore.save(groupId: groupId, descriptor: descriptor)
            }
        } else {
            try secretStore.save(groupId: groupId, descriptor: descriptor)
        }

        let payment = IncomingPayment(
            paymentId: paymentId,
            productId: productId,
            amount: amount,
            createdAt: Date(),
            outcome: nil,
            ownerId: ownerId
        )
        do {
            try await store.save(payment, authorization: authorization)
        } catch {
            if let lifecycle, let operation {
                try? lifecycle.withEffect(operation) { secretStore.remove(groupId: groupId) }
            } else {
                secretStore.remove(groupId: groupId)
            }
            throw error
        }
    }

    /// Throws `SourceBusy` when the descriptor draws on the same funds as an active payment's. An
    /// active payment whose secret is corrupted can never claim, so it cannot hold funds busy; a
    /// store that cannot be read at all still aborts, since nothing is known about those payments.
    func ensureSourceFree(descriptor: IncomingPaymentSourceDescriptor) async throws {
        let active = try await store.fetchActivePayments()
        for other in active where other.ownerId == ownerId {
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
                try lifecycle?.checkCurrentOperation()
                for payment in payments where payment.ownerId == ownerId {
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

        do {
            guard let current = try await store.fetch(groupId: payment.groupId),
                  current.ownerId == ownerId, current.isActive else { return }
            try lifecycle?.checkCurrentOperation()
            switch lookupSecret(for: current) {
            case .unreadable:
                return
            case .gone:
                // Can't re-run without the source, so settle from the ledger alone.
                await settleFromDurability(payment: current, denomination: denomination)
            case let .found(descriptor):
                await runClaim(payment: current, descriptor: descriptor, denomination: denomination)
            }
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) cannot resume: \(error)")
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
                last = IncomingPaymentStatus(detection: detection, amount: payment.amount)
                try lifecycle?.checkCurrentOperation()
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
            let operation = try lifecycle?.captureOperation()
            try await store.settle(groupId: payment.groupId, ownerId: ownerId, outcome: outcome) { [lifecycle] in
                if let lifecycle, let operation { try lifecycle.check(operation) }
            }
            // A readable secret is retired only by a verdict that says where the funds went. `notClaimed`
            // says they were never collected — and for a `.coins` source the stored secrets *are* the
            // money, so removing them here is what turns an uncollected top-up into an unrecoverable one.
            // Keeping them costs a Keychain entry; a retention sweep can reclaim those once one exists.
            if !secretIsUsable || outcome != .notClaimed {
                if let lifecycle, let operation {
                    try lifecycle.withEffect(operation) { secretStore.remove(groupId: payment.groupId) }
                } else {
                    secretStore.remove(groupId: payment.groupId)
                }
            }
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) settlement interrupted: \(error)")
            return
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
            let status = IncomingPaymentStatus(detection: detection, amount: payment.amount)
            try lifecycle?.checkCurrentOperation()
            await paymentContext.report(status, for: payment.groupId)
            // The secret is gone or corrupt — that is why this path was taken — so it is retired with
            // the verdict whatever that verdict is.
            await settle(payment: payment, finalStatus: status, secretIsUsable: false)
        } catch {
            logger?.error("Incoming payment \(payment.paymentId) group unobservable; left for next launch: \(error)")
        }
    }
}
