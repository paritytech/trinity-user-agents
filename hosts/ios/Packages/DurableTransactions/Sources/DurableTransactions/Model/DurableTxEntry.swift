import Foundation
import Operation_iOS

public typealias DurableTxId = UUID

/// Groups the transactions registered together by one operation (e.g. a transfer's message id).
/// Supplied by the caller; `nil` for an ungrouped single submission.
public typealias DurableTxGroupId = String

/// Which consumer a transaction belongs to. One ledger serves every domain, and a recovery pass
/// evaluates each domain against its own ``TxCompletionOracle``.
public struct TxDomainId: RawRepresentable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.rawValue = rawValue
    }

    public var description: String { rawValue }
}

/// What the engine knows about one transaction.
///
/// Deliberately thin: the bytes' hash, the window they are valid in, and what the ledger has concluded
/// so far. Anything domain-shaped reaches the rules only through ``TxCompletionOracle``.
public struct DurableTxEntry: Sendable, Equatable {
    public let id: DurableTxId
    public let domainId: TxDomainId

    /// Registration order. Monotonic, assigned by the store; recovery evaluates in this order so a
    /// transaction depending on another is never decided before its predecessor.
    public let sequence: Int64

    /// The operation that registered this transaction, `nil` for an ungrouped single submission.
    /// A label only: no rule reads it. It lets an operation's transactions be found together.
    public let groupId: DurableTxGroupId?

    /// The bytes currently in flight and the window they can land in.
    ///
    /// `nil` for a transaction that has been scheduled but not built yet, and for a stored row whose
    /// attempt columns cannot be read back. Nothing a rule needs is knowable without it, which is why
    /// the ladder treats a missing attempt as undecided rather than guessing.
    public let attempt: DurableTxAttempt?

    /// Block where execution was first observed. Only ever written where success is already proven, so
    /// Rule 0 need only re-check that the block is still canonical.
    public let successDetectedAt: BlockRef?

    public let status: DurableTxStatus

    public let createdAt: Date

    public init(
        id: DurableTxId = UUID(),
        domainId: TxDomainId,
        sequence: Int64 = 0,
        groupId: DurableTxGroupId? = nil,
        attempt: DurableTxAttempt?,
        successDetectedAt: BlockRef? = nil,
        status: DurableTxStatus = .pending,
        createdAt: Date = Date()
    ) {
        self.id = id
        self.domainId = domainId
        self.sequence = sequence
        self.groupId = groupId
        self.attempt = attempt
        self.successDetectedAt = successDetectedAt
        self.status = status
        self.createdAt = createdAt
    }

    /// Convenience for a caller that already has the three parts of an attempt.
    public init(
        id: DurableTxId = UUID(),
        domainId: TxDomainId,
        sequence: Int64 = 0,
        groupId: DurableTxGroupId? = nil,
        txHash: Data,
        checkpoint: BlockRef,
        mortality: UInt32,
        successDetectedAt: BlockRef? = nil,
        status: DurableTxStatus = .pending,
        createdAt: Date = Date()
    ) {
        self.init(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: DurableTxAttempt(
                txHash: txHash,
                checkpoint: checkpoint,
                mortalityBlocks: mortality
            ),
            successDetectedAt: successDetectedAt,
            status: status,
            createdAt: createdAt
        )
    }
}

extension DurableTxEntry: Operation_iOS.Identifiable {
    public var identifier: String {
        id.uuidString
    }
}

public extension DurableTxEntry {
    var txHash: Data? { attempt?.txHash }
    var checkpoint: BlockRef? { attempt?.checkpoint }
    var mortality: UInt32? { attempt?.mortalityBlocks }

    /// A copy with `status` replaced — the entry is immutable, so a status write rebuilds it.
    func withStatus(_ status: DurableTxStatus) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: attempt,
            successDetectedAt: successDetectedAt,
            status: status,
            createdAt: createdAt
        )
    }

    /// Replaces the attempt in place and makes the row `pending`, clearing whatever the previous attempt
    /// recorded — a new attempt has observed nothing yet. The id, sequence and group are kept, which is
    /// what leaves the domain's locked inputs and outputs attached across a rebuild.
    func withAttempt(_ attempt: DurableTxAttempt) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: attempt,
            successDetectedAt: nil,
            status: .pending,
            createdAt: createdAt
        )
    }

    /// A copy with `successDetectedAt` replaced (`nil` clears the record).
    func withSuccessDetectedAt(_ block: BlockRef?) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: attempt,
            successDetectedAt: block,
            status: status,
            createdAt: createdAt
        )
    }

    /// A copy with `sequence` replaced — for a store assigning the order on insert.
    func withSequence(_ sequence: Int64) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: attempt,
            successDetectedAt: successDetectedAt,
            status: status,
            createdAt: createdAt
        )
    }
}

/// A transaction ready to be recorded: its first attempt, and the policy that may build it again.
/// Carries no id — the repository mints the ``DurableTxId`` inside the write transaction.
public struct DurableTxRegistration: Sendable, Equatable {
    public let domainId: TxDomainId
    public let groupId: DurableTxGroupId?
    public let attempt: DurableTxAttempt

    /// Builds this transaction again once an attempt is proven unable to land; `nil` for one whose
    /// failure is final.
    public let policy: SubmissionPolicy?

    public init(
        domainId: TxDomainId,
        groupId: DurableTxGroupId?,
        attempt: DurableTxAttempt,
        policy: SubmissionPolicy? = nil
    ) {
        self.domainId = domainId
        self.groupId = groupId
        self.attempt = attempt
        self.policy = policy
    }

    public init(
        domainId: TxDomainId,
        groupId: DurableTxGroupId?,
        txHash: Data,
        checkpoint: BlockRef,
        mortalityBlocks: UInt32,
        policy: SubmissionPolicy? = nil
    ) {
        self.init(
            domainId: domainId,
            groupId: groupId,
            attempt: DurableTxAttempt(
                txHash: txHash,
                checkpoint: checkpoint,
                mortalityBlocks: mortalityBlocks
            ),
            policy: policy
        )
    }
}

public extension DurableTxRegistration {
    var txHash: Data { attempt.txHash }
    var checkpoint: BlockRef { attempt.checkpoint }
    var mortalityBlocks: UInt32 { attempt.mortalityBlocks }

    /// Builds the entry the repository stores — `id` minted by the store, `sequence` the next in order,
    /// status `.pending`.
    func makeEntry(id: DurableTxId, sequence: Int64) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: attempt,
            status: .pending
        )
    }
}

/// A transaction to be recorded with no attempt yet: its policy builds and submits it afterwards.
///
/// What it will consume is locked from the moment this commits, so nothing else can select those
/// assets while it waits to be built.
public struct DurableTxSchedule: Sendable, Equatable {
    public let domainId: TxDomainId
    public let groupId: DurableTxGroupId?
    public let policy: SubmissionPolicy

    public init(domainId: TxDomainId, groupId: DurableTxGroupId?, policy: SubmissionPolicy) {
        self.domainId = domainId
        self.groupId = groupId
        self.policy = policy
    }
}

public extension DurableTxSchedule {
    /// Builds the row the repository stores for a transaction that has not been built yet.
    ///
    /// It carries no attempt — a scheduled row has no bytes, no anchor and no window — until
    /// ``DurableTxEntry/withAttempt(_:)`` gives it one. Nothing needs one while the status is
    /// ``DurableTxStatus/pendingSubmission``: the recovery pass filters on
    /// ``DurableTxStatus/awaitsVerdict``, and a store never returns such a row from
    /// `getAllEntries(domain:)`. The row is still a ``DurableTxEntry`` so a caller watching its operation
    /// group sees the transaction exist and waits for it, rather than reading an empty group as a
    /// finished one.
    func makeEntry(id: DurableTxId, sequence: Int64) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            sequence: sequence,
            groupId: groupId,
            attempt: nil,
            status: .pendingSubmission
        )
    }
}
