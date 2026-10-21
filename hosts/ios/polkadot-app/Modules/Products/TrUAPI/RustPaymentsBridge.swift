import Foundation
import AsyncExtensions
import BigInt
import Coinage
import Products
import TrUAPIHost

/// Serves the Rust core's top-up, payment and balance platforms from the engines the JS bridge
/// uses, through the same ``ProductPayments`` steps. Installed once on the shared runtime.
///
/// The core checks the session, the source keys and balance access itself, so none of that is
/// repeated here, and a short balance is always `InsufficientBalance`: the core turns it into
/// `Rejected` for a product without balance access. Amounts arrive as decimal strings, so they are
/// still parsed here, and one that does not parse to a positive number is `InvalidAmount`.
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
            notify: { [target, logger] key, status in
                do {
                    try target.runtime?.notifyTopUpStatus(productId: key.productId, id: key.id, status: status)
                } catch {
                    logger.error("[truapi] top-up status could not be reported: \(error)")
                }
            },
            pending: .detecting,
            abandoned: .notClaimed,
            logger: logger
        )
        paymentStatuses = EngineStatusFollowers(
            statuses: { [payments] key in
                payments.paymentStatuses(productId: key.productId, id: key.id)
                    .map { ($0.wireStatus, $0.wireStatus.isFinal) }
                    .eraseToAnyAsyncSequence()
            },
            isNotFound: { ($0 as? ExternalPaymentError) == .notFound },
            notify: { [target, logger] key, status in
                do {
                    try target.runtime?.notifyPaymentStatus(productId: key.productId, id: key.id, status: status)
                } catch {
                    logger.error("[truapi] payment status could not be reported: \(error)")
                }
            },
            pending: .processing,
            abandoned: .failed(reason: Self.paymentDidNotComplete),
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
        guard let amount = BigUInt(request.amount), amount > 0 else {
            throw TrUAPIHostPaymentTopUpError.InvalidAmount
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
        guard let amount = BigUInt(request.amount), amount > 0 else {
            throw HostPaymentError.InvalidAmount
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
    /// One balance subscription for the bridge's lifetime, ended when the bridge is released.
    /// Whenever the stream ends or fails (the balance service is unavailable until Coinage is set
    /// up), it subscribes again after ``balanceResubscribeDelay``.
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

    static let balanceResubscribeDelay: Duration = .seconds(5)
}

// MARK: - Wire reasons

private extension RustPaymentsBridge {
    /// What a product is told on an unclassified failure. The real error is logged; a CoreData or
    /// Keychain dump is not for product scripts.
    static let topUpRegistrationFailed = "top-up could not be registered"
    static let paymentFailed = "payment could not be started"
    static let balanceUnavailable = "balance is unavailable"
    static let onlyMainPurse = "only the main purse is supported"
    static let paymentDidNotComplete = "payment did not complete"
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

struct EngineStatusKey: Hashable, Sendable {
    let productId: String
    let id: Data
}

/// Limits of ``EngineStatusFollowers``, which as a generic type cannot hold them.
enum EngineStatusFollowing {
    /// How long a status read waits for the engine's first answer on an id nobody follows yet. The
    /// read is synchronous on one of the core's worker threads, so it waits briefly and then answers
    /// the pending status, pushing the real one when the engine sends it.
    static let firstStatusTimeout: TimeInterval = 0.25
    /// How many ids' final statuses are kept.
    static let finalCapacity = 512
}

/// Follows an engine's status streams, at most one per id, remembering each id's latest status so
/// the core's synchronous "current status" read can be answered. A final status is kept, for the
/// most recent ``EngineStatusFollowing/finalCapacity`` ids, so reading it again answers at once.
/// A read that finds no status yet waits at most the timeout, then answers `pending`; the engine's
/// statuses are pushed as they arrive.
///
/// Following ends at a final status, or when the engine's stream ends or fails. Once the core has
/// been told a status that is not final, by a push or a `pending` read, it waits for a final one,
/// so a stream ending without one pushes `abandoned`. That status is not kept: the entry is
/// dropped, and the core's next read follows the engine afresh, answering its record or `nil`.
final class EngineStatusFollowers<Status>: @unchecked Sendable {
    /// A stream of statuses, each with whether it is final.
    typealias Statuses = (EngineStatusKey) async throws -> AnyAsyncSequence<(Status, Bool)>

    final class Entry {
        var latest: Status?
        /// Set once the first status arrived or the stream ended.
        var settled = false
        var failed = false
        var isFinal = false
        /// Set once the core was told a status, pushed or read.
        var reported = false
    }

    private let statuses: Statuses
    private let isNotFound: (Error) -> Bool
    private let notify: (EngineStatusKey, Status) -> Void
    private let pending: Status
    private let abandoned: Status
    private let logger: LoggerProtocol
    private let timeout: TimeInterval
    private let finalCapacity: Int

    private let condition = NSCondition()
    private var entries: [EngineStatusKey: Entry] = [:]
    private var finalKeys: [EngineStatusKey] = []

    init(
        statuses: @escaping Statuses,
        isNotFound: @escaping (Error) -> Bool,
        notify: @escaping (EngineStatusKey, Status) -> Void,
        pending: Status,
        abandoned: Status,
        logger: LoggerProtocol,
        timeout: TimeInterval = EngineStatusFollowing.firstStatusTimeout,
        finalCapacity: Int = EngineStatusFollowing.finalCapacity
    ) {
        self.statuses = statuses
        self.isNotFound = isNotFound
        self.notify = notify
        self.pending = pending
        self.abandoned = abandoned
        self.logger = logger
        self.timeout = timeout
        self.finalCapacity = finalCapacity
    }

    /// Start following `key` unless it already is.
    func follow(_ key: EngineStatusKey) {
        _ = entry(for: key)
    }

    /// The latest status of `key`, following it when nobody does. With no status yet it waits at most
    /// the timeout for the engine's first answer, then answers `pending`. `nil` when the engine holds
    /// no such id. An engine that fails is reported as the status being unavailable; the reason is
    /// logged, never handed to the product.
    func current(_ key: EngineStatusKey) throws -> Status? {
        let entry = entry(for: key)

        condition.lock()
        defer { condition.unlock() }

        let deadline = Date(timeIntervalSinceNow: timeout)
        while !entry.settled {
            guard condition.wait(until: deadline) else {
                entry.reported = true
                return pending
            }
        }

        if let latest = entry.latest { return latest }
        if entry.failed { throw HostRejection.Rejected(reason: "status is unavailable") }
        return nil
    }

    /// Whether `key` is followed, or its final status kept. Lets tests observe eviction.
    func isFollowing(_ key: EngineStatusKey) -> Bool {
        condition.lock()
        defer { condition.unlock() }
        return entries[key] != nil
    }
}

private extension EngineStatusFollowers {
    /// The entry following `key`, starting one when there is none. Each status is remembered, then
    /// pushed. Following stops at a final status or when the stream ends or fails. An entry with a
    /// final status is kept, up to ``EngineStatusFollowing/finalCapacity`` of them; any other entry
    /// is dropped, so a later read follows afresh from the engine's record.
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
                for try await (status, isFinal) in try await statuses(key) {
                    settle(entry) {
                        $0.latest = status
                        $0.isFinal = isFinal
                        $0.reported = true
                    }
                    notify(key, status)
                    if isFinal { break }
                }
                finish(key, entry, failed: false)
            } catch {
                let notFound = isNotFound(error)
                if !notFound {
                    logger.error("[truapi] status stream failed: \(error)")
                }
                finish(key, entry, failed: !notFound)
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

    func finish(_ key: EngineStatusKey, _ entry: Entry, failed: Bool) {
        var abandon = false
        settle(entry) { entry in
            if entry.latest == nil {
                entry.failed = failed
            }
            abandon = entry.reported && !entry.isFinal
        }

        // Pushed before the entry is dropped, so a later follow's statuses cannot reach the core first.
        if abandon {
            notify(key, abandoned)
        }
        forget(key, entry)
    }

    func forget(_ key: EngineStatusKey, _ entry: Entry) {
        condition.lock()
        defer { condition.unlock() }
        guard entries[key] === entry else { return }

        if entry.isFinal {
            finalKeys.append(key)
            if finalKeys.count > finalCapacity {
                entries[finalKeys.removeFirst()] = nil
            }
        } else {
            entries[key] = nil
        }
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
        case .invalidAmount: self = .InvalidAmount
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
        case let .partiallyCompleted(settled): .claimedPartially(actualClaimed: String(settled))
        // The engine's reason can name a shortfall the core masks; products get a fixed one.
        case .failed: .failed(reason: RustPaymentsBridge.paymentDidNotComplete)
        }
    }
}

private extension HostPaymentStatusSubscribeItem {
    /// `Completed`, `Failed` and `ClaimedPartially` do not change again.
    var isFinal: Bool {
        switch self {
        case .processing: false
        case .completed,
             .failed,
             .claimedPartially: true
        }
    }
}
