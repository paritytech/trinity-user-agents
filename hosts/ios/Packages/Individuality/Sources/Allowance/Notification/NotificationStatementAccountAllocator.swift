import AsyncExtensions
import DurableTransactions
import Foundation
import SDKLogger
import StructuredConcurrency
import SubstrateSdk

/// Funds statement accounts from anonymous notification slots: each claim lets its account keep one
/// statement until the current period ends plus the runtime's grace window. An account holds one live
/// binding, so callers pass a fresh account per period.
public protocol NotificationStatementAccountAllocating: Sendable {
    /// The period slots are claimed in right now.
    func currentPeriod() async throws -> UInt32

    func maxStatementSize() async throws -> Int

    /// Reserves a free slot of the current period for each target and schedules its durable claim, in order
    /// of preference: when fewer slots are free than targets, only the leading ones are scheduled. A target
    /// whose claim is already live is left as is. Returns once the claims are recorded, not when they land:
    /// the targets scheduled or already claimed.
    func initiateAllocations(for targets: [AccountId]) async throws -> [AccountId]

    /// Waits until a claim for `target` has executed (finality is not required), up to `timeout`.
    func awaitAllocated(_ target: AccountId, timeout: Duration) async throws
}

public extension NotificationStatementAccountAllocating {
    func initiateAllocation(for target: AccountId) async throws {
        let initiated = try await initiateAllocations(for: [target])

        guard initiated.contains(target) else {
            throw NotificationAllocationError.noFreeSlotInPeriod
        }
    }

    func allocate(_ target: AccountId, timeout: Duration) async throws {
        try await initiateAllocation(for: target)
        try await awaitAllocated(target, timeout: timeout)
    }
}

public enum NotificationAllocationError: Error, Equatable {
    case noFreeSlotInPeriod
    case timeout
}

/// Where claims are recorded: the durable ledger, written inside a store transaction the app owns.
public protocol NotificationClaimLedger: Sendable {
    func scheduleClaim(groupId: DurableTxGroupId, policy: SubmissionPolicy) async throws
    func claims(groupId: DurableTxGroupId) async throws -> [DurableTxEntry]
    func subscribeClaims(groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]>
}

/// Where free slots are read from.
public struct NotificationSlotSources: @unchecked Sendable {
    let repository: NotificationSlotRepositoryProtocol
    let originPersonProvider: OriginPersonProviding
    let networkSuffixProvider: NetworkSuffixProviding

    public init(
        repository: NotificationSlotRepositoryProtocol,
        originPersonProvider: OriginPersonProviding,
        networkSuffixProvider: NetworkSuffixProviding
    ) {
        self.repository = repository
        self.originPersonProvider = originPersonProvider
        self.networkSuffixProvider = networkSuffixProvider
    }
}

/// State shared by the allocator and the submission policy, which must agree on reservations and take
/// the same lock for every read-then-claim of a seq.
public struct NotificationSlotDependencies: @unchecked Sendable {
    let chainId: ChainId
    let picker: NotificationSeqPicker
    let reservations: NotificationSeqReservations
    let serialQueue: SerialOperationQueue
    let parameters: NotificationParametersProviding
    let issueDiagnostics: NotificationSlotIssueDiagnostics
    let logger: SDKLoggerProtocol

    public init(
        chainId: ChainId,
        sources: NotificationSlotSources,
        parameters: NotificationParametersProviding,
        issueDiagnostics: NotificationSlotIssueDiagnostics,
        logger: SDKLoggerProtocol
    ) {
        let reservations = NotificationSeqReservations()

        self.chainId = chainId
        picker = NotificationSeqPicker(
            sources: sources,
            reservations: reservations,
            unsupportedRuntime: issueDiagnostics.unsupportedRuntime,
            logger: logger
        )
        self.reservations = reservations
        serialQueue = SerialOperationQueue()
        self.parameters = parameters
        self.issueDiagnostics = issueDiagnostics
        self.logger = logger
    }
}

public final class NotificationStatementAccountAllocator: @unchecked Sendable {
    private let ledger: NotificationClaimLedger
    private let picker: NotificationSeqPicker
    private let reservations: NotificationSeqReservations
    private let serialQueue: SerialOperationQueue
    private let parameters: NotificationParametersProviding
    private let clock: any Clock<Duration>
    private let logger: SDKLoggerProtocol

    public init(
        dependencies: NotificationSlotDependencies,
        ledger: NotificationClaimLedger,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        picker = dependencies.picker
        reservations = dependencies.reservations
        serialQueue = dependencies.serialQueue
        parameters = dependencies.parameters
        logger = dependencies.logger
        self.ledger = ledger
        self.clock = clock
    }
}

extension NotificationStatementAccountAllocator: NotificationStatementAccountAllocating {
    public func currentPeriod() async throws -> UInt32 {
        try await parameters.currentPeriod()
    }

    public func maxStatementSize() async throws -> Int {
        try await parameters.maxStatementSize()
    }

    public func initiateAllocations(for targets: [AccountId]) async throws -> [AccountId] {
        try await serialQueue.run { [self] in
            let claimed = try await liveClaims(among: targets)
            let unclaimed = targets.filter { !claimed.contains($0) }
            let period = try await parameters.currentPeriod()
            let free = try await picker.freeSlots(period: period, forTarget: nil)

            var scheduled = Set<AccountId>()
            for (target, slot) in zip(unclaimed, free) {
                try await schedule(slot, for: target)
                scheduled.insert(target)
            }

            logger.info(
                "Notification claims of period \(period): \(claimed.count) live, \(scheduled.count) scheduled, " +
                    "\(unclaimed.count - scheduled.count) left without a slot"
            )

            return targets.filter { claimed.contains($0) || scheduled.contains($0) }
        }
    }

    public func awaitAllocated(_ target: AccountId, timeout: Duration) async throws {
        let claims = ledger.subscribeClaims(groupId: NotificationSlotDomain.groupId(for: target))

        do {
            try await withTimeout(timeout, clock: clock) {
                for try await entries in claims {
                    if entries.contains(where: \.status.isArrived) {
                        return
                    }

                    if !entries.isEmpty, !entries.contains(where: \.status.canArrive) {
                        throw NotificationAllocationError.noFreeSlotInPeriod
                    }
                }

                throw NotificationAllocationError.timeout
            }
        } catch is TimeoutError {
            throw NotificationAllocationError.timeout
        }
    }
}

private extension NotificationStatementAccountAllocator {
    func liveClaims(among targets: [AccountId]) async throws -> Set<AccountId> {
        var live = Set<AccountId>()

        for target in targets {
            let claims = try await ledger.claims(groupId: NotificationSlotDomain.groupId(for: target))

            if claims.contains(where: \.status.canArrive) {
                live.insert(target)
            }
        }

        return live
    }

    // Reserved before the claim is recorded: it is built later, and until then the slot is ours only here.
    func schedule(_ slot: NotificationSlot, for target: AccountId) async throws {
        try reservations.reserve(slot.key, for: target)

        do {
            try await ledger.scheduleClaim(
                groupId: NotificationSlotDomain.groupId(for: target),
                policy: NotificationSlotDomain.policy(for: target)
            )
        } catch {
            reservations.release(target)
            throw error
        }
    }
}
