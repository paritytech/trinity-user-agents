import Foundation
import AsyncExtensions
import BigInt
import Coinage
import Products
import TrUAPIHost

/// Serves the Rust core's top-up, payment and balance platforms from the engines the JS bridge
/// uses, through the same ``ProductPayments`` steps. Installed once on the shared runtime.
///
/// The core checks the session, the source keys, a positive amount and balance access itself, so
/// none of that is repeated here, and a short balance is always `InsufficientBalance`: the core
/// turns it into `Rejected` for a product without balance access.
///
/// The core asks a top-up's or payment's current status once, then expects every later one pushed.
/// So each id is followed, once, on its engine's status stream from the first time the core asks
/// about it or the product starts it, and each status is pushed until a terminal one. Asking after
/// a restart re-reads the engine, which keeps its records.
final class RustPaymentsBridge: TopUpHostBridge, PaymentHostBridge, BalanceHostBridge, @unchecked Sendable {
    private let payments: ProductPayments
    private let logger: LoggerProtocol
    private let target: RuntimeTarget
    private let topUps: EngineStatusFollowers<HostPaymentTopUpStatusSubscribeItem>
    private let paymentStatuses: EngineStatusFollowers<HostPaymentStatusSubscribeItem>

    private let lock = NSLock()
    private var balanceTask: Task<Void, Never>?

    init(payments: ProductPayments, logger: LoggerProtocol) {
        let target = RuntimeTarget()
        self.payments = payments
        self.logger = logger
        self.target = target

        topUps = EngineStatusFollowers(
            statuses: { [payments] key in
                try await payments.topUpStatuses(productId: key.productId, id: key.id)
                    .map { ($0.wireStatus, $0.isTerminal) }
                    .eraseToAnyAsyncSequence()
            },
            isNotFound: { error in
                if case .notFound = error as? IncomingPaymentError { return true }
                return false
            },
            notify: { [target] key, status in
                target.runtime?.notifyTopUpStatus(productId: key.productId, id: key.id, status: status)
            },
            logger: logger
        )
        paymentStatuses = EngineStatusFollowers(
            statuses: { [payments] key in
                // The engine ends the stream after its first terminal status.
                payments.paymentStatuses(productId: key.productId, id: key.id)
                    .map { ($0.wireStatus, false) }
                    .eraseToAnyAsyncSequence()
            },
            isNotFound: { ($0 as? ExternalPaymentError) == .notFound },
            notify: { [target] key, status in
                target.runtime?.notifyPaymentStatus(productId: key.productId, id: key.id, status: status)
            },
            logger: logger
        )
    }

    deinit {
        balanceTask?.cancel()
    }

    /// Serve `runtime`'s payment platforms and start pushing balance changes to it.
    func install(on runtime: TrUAPIHostRuntime) {
        target.runtime = runtime

        runtime.setTopUp(self)
        runtime.setPayments(self)
        runtime.setBalance(self)
        followBalance()
    }

    // MARK: - TopUpHostBridge

    func topUp(productId: String, request: HostPaymentTopUpRequest) async throws {
        guard request.into == nil else {
            throw TrUAPIHostPaymentTopUpError.Unknown(reason: Self.onlyMainPurse)
        }
        guard let amount = BigUInt(request.amount) else {
            throw TrUAPIHostPaymentTopUpError.Unknown(reason: Self.invalidAmount)
        }
        guard let source = try? request.source.toAppSource() else {
            throw TrUAPIHostPaymentTopUpError.InvalidSource
        }

        do {
            try await payments.acceptTopUp(productId: productId, id: request.id, amount: amount, source: source)
        } catch {
            logger.error("[truapi] top-up could not be registered: \(error)")
            throw TrUAPIHostPaymentTopUpError(error, unknownReason: Self.topUpRegistrationFailed)
        }

        topUps.follow(EngineStatusKey(productId: productId, id: request.id))
    }

    func topUpStatus(productId: String, id: Data) throws -> HostPaymentTopUpStatusSubscribeItem? {
        try topUps.current(EngineStatusKey(productId: productId, id: id))
    }

    // MARK: - PaymentHostBridge

    func requestPayment(productId: String, request: HostPaymentRequest) async throws {
        guard request.from == nil else {
            throw HostPaymentError.Unknown(reason: Self.onlyMainPurse)
        }
        guard let amount = BigUInt(request.amount) else {
            throw HostPaymentError.Unknown(reason: Self.invalidAmount)
        }

        do {
            guard try await payments.canSpend(amount) else {
                throw HostPaymentError.InsufficientBalance
            }
            guard try await payments.awaitUserConsent(
                productId: productId,
                amount: amount,
                destination: request.destination
            ) else {
                throw HostPaymentError.Rejected
            }
            try await payments.initiatePayment(
                productId: productId,
                id: request.id,
                amount: amount,
                destination: request.destination
            )
        } catch let error as HostPaymentError {
            throw error
        } catch ExternalPaymentError.alreadyExists {
            throw HostPaymentError.AlreadyExists
        } catch {
            logger.error("[truapi] payment could not be started: \(error)")
            throw HostPaymentError.Unknown(reason: Self.paymentFailed)
        }

        paymentStatuses.follow(EngineStatusKey(productId: productId, id: request.id))
    }

    func paymentStatus(productId: String, id: Data) throws -> HostPaymentStatusSubscribeItem? {
        try paymentStatuses.current(EngineStatusKey(productId: productId, id: id))
    }

    // MARK: - BalanceHostBridge

    func balance(productId _: String, purse: UInt32?) async throws -> U128 {
        guard purse == nil else {
            throw HostPaymentBalanceSubscribeError.Unknown(reason: Self.onlyMainPurse)
        }

        do {
            for try await balance in try await payments.balanceStream().prefix(1) {
                return String(balance.spendableByPayment)
            }
        } catch {
            logger.error("[truapi] balance is unavailable: \(error)")
        }
        throw HostPaymentBalanceSubscribeError.Unknown(reason: Self.balanceUnavailable)
    }
}

// MARK: - Balance

private extension RustPaymentsBridge {
    /// One subscription for the runtime's lifetime, resubscribed after a failure (the balance
    /// service is unavailable until Coinage is set up).
    func followBalance() {
        lock.lock()
        defer { lock.unlock() }
        guard balanceTask == nil else { return }

        balanceTask = Task { [payments, target, logger] in
            while !Task.isCancelled {
                do {
                    for try await balance in try await payments.balanceStream() {
                        target.runtime?.notifyBalance(purse: nil, available: String(balance.spendableByPayment))
                    }
                } catch {
                    logger.warning("[truapi] balance stream failed, resubscribing: \(error)")
                }
                try? await Task.sleep(for: Self.balanceResubscribeDelay)
            }
        }
    }
}

// MARK: - Wire reasons

private extension RustPaymentsBridge {
    /// What a product is told on an unclassified failure. The real error is logged; a CoreData or
    /// Keychain dump is not for product scripts.
    static let topUpRegistrationFailed = "top-up could not be registered"
    static let paymentFailed = "payment could not be started"
    static let balanceUnavailable = "balance is unavailable"
    static let onlyMainPurse = "only the main purse is supported"
    static let invalidAmount = "amount is not a decimal number"
    static let balanceResubscribeDelay: Duration = .seconds(5)
}

// MARK: - Runtime target

/// The runtime statuses are pushed to, held weakly: the runtime retains this bridge.
private final class RuntimeTarget: @unchecked Sendable {
    private let lock = NSLock()
    private weak var storedRuntime: TrUAPIHostRuntime?

    var runtime: TrUAPIHostRuntime? {
        get {
            lock.lock()
            defer { lock.unlock() }
            return storedRuntime
        }
        set {
            lock.lock()
            storedRuntime = newValue
            lock.unlock()
        }
    }
}

// MARK: - Engine status followers

private struct EngineStatusKey: Hashable, Sendable {
    let productId: String
    let id: Data
}

/// Follows an engine's status streams, at most one per id, remembering each id's latest status so
/// the core's synchronous "current status" read can be answered.
private final class EngineStatusFollowers<Status>: @unchecked Sendable {
    /// A stream of statuses, each with whether it is terminal.
    typealias Statuses = (EngineStatusKey) async throws -> AnyAsyncSequence<(Status, Bool)>

    /// How long a status read waits for the engine's first answer on an id nobody follows yet. The
    /// read is synchronous on the core's thread; the engines answer from their stores.
    private static var firstStatusTimeout: TimeInterval { 5 }

    final class Entry {
        var latest: Status?
        /// Set once the first status arrived or the stream ended.
        var settled = false
        var failure: Error?
    }

    private let statuses: Statuses
    private let isNotFound: (Error) -> Bool
    private let notify: (EngineStatusKey, Status) -> Void
    private let logger: LoggerProtocol

    private let condition = NSCondition()
    private var entries: [EngineStatusKey: Entry] = [:]

    init(
        statuses: @escaping Statuses,
        isNotFound: @escaping (Error) -> Bool,
        notify: @escaping (EngineStatusKey, Status) -> Void,
        logger: LoggerProtocol
    ) {
        self.statuses = statuses
        self.isNotFound = isNotFound
        self.notify = notify
        self.logger = logger
    }

    /// Start following `key` unless it already is.
    func follow(_ key: EngineStatusKey) {
        _ = entry(for: key)
    }

    /// The latest status of `key`, following it when nobody does and waiting for the engine's first
    /// answer if none arrived yet. `nil` when the engine holds no such id.
    func current(_ key: EngineStatusKey) throws -> Status? {
        let entry = entry(for: key)

        condition.lock()
        defer { condition.unlock() }

        let deadline = Date(timeIntervalSinceNow: Self.firstStatusTimeout)
        while !entry.settled {
            guard condition.wait(until: deadline) else {
                logger.error("[truapi] no status within \(Self.firstStatusTimeout)s")
                throw HostRejection.Rejected(reason: "status is unavailable")
            }
        }

        if let latest = entry.latest { return latest }
        if let failure = entry.failure { throw failure }
        return nil
    }
}

private extension EngineStatusFollowers {
    /// The entry following `key`, starting one when there is none. Each status is remembered, then
    /// pushed; following ends after a terminal status or when the stream ends, and the entry is
    /// dropped so a later read follows afresh from the engine's record.
    func entry(for key: EngineStatusKey) -> Entry {
        condition.lock()
        if let existing = entries[key] {
            condition.unlock()
            return existing
        }
        let entry = Entry()
        entries[key] = entry
        condition.unlock()

        Task { [self] in
            do {
                for try await (status, terminal) in try await statuses(key) {
                    settle(entry) { $0.latest = status }
                    notify(key, status)
                    if terminal { break }
                }
                finish(key, entry, failure: nil)
            } catch {
                let notFound = isNotFound(error)
                if !notFound {
                    logger.error("[truapi] status stream failed: \(error)")
                }
                finish(key, entry, failure: notFound ? nil : error)
            }
        }

        return entry
    }

    func settle(_ entry: Entry, _ update: (Entry) -> Void) {
        condition.lock()
        update(entry)
        entry.settled = true
        condition.broadcast()
        condition.unlock()
    }

    func finish(_ key: EngineStatusKey, _ entry: Entry, failure: Error?) {
        settle(entry) { entry in
            if entry.latest == nil {
                entry.failure = failure
            }
        }

        condition.lock()
        if entries[key] === entry {
            entries[key] = nil
        }
        condition.unlock()
    }
}

// MARK: - Wire mapping

private extension TrUAPIHostPaymentTopUpSource {
    func toAppSource() throws -> Products.PaymentTopUpSource {
        switch self {
        case let .productAccount(derivationIndex):
            try .productAccount(derivationIndex: derivationIndex.toSelector())
        case let .privateKey(secretKey):
            .privateKey(secretKey)
        case let .coins(secretKeys):
            .coins(secretKeys: secretKeys)
        }
    }
}

private extension TrUAPIHostPaymentTopUpError {
    /// The coded error for `error`; anything that is not a classified `IncomingPaymentError` becomes
    /// `Unknown` with the generic `unknownReason` rather than the error's own description.
    init(_ error: any Error, unknownReason: String) {
        if error is ProductTopUpSourceError {
            self = .InvalidSource
            return
        }
        switch error as? IncomingPaymentError {
        case .alreadyExists: self = .AlreadyExists
        case .invalidSource: self = .InvalidSource
        case .sourceBusy: self = .SourceBusy
        case .invalidAmount: self = .Unknown(reason: "amount must be positive")
        case .notFound,
             .unknown,
             .none: self = .Unknown(reason: unknownReason)
        }
    }
}

private extension IncomingPaymentStatus {
    var wireStatus: HostPaymentTopUpStatusSubscribeItem {
        switch self {
        case .detecting: .detecting
        case .claiming: .claiming
        case let .claimed(finalized): .claimed(finalized: finalized)
        case let .claimedPartially(actualClaimed): .claimedPartially(actualClaimed: String(actualClaimed))
        case .notClaimed: .notClaimed
        }
    }
}

private extension ExternalPaymentStatus {
    var wireStatus: HostPaymentStatusSubscribeItem {
        switch self {
        case .processing: .processing
        case .completed: .completed
        case let .partiallyCompleted(settled): .partiallyClaimed(actualClaimed: String(settled))
        case let .failed(reason): .failed(reason: reason)
        }
    }
}
