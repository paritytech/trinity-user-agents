import AsyncExtensions
import Foundation

/// Marker for the store's open write transaction.
///
/// A domain writes its own rows through its own store from inside a hook receiving this, so those rows
/// and the engine's row commit together. Throwing from the hook rolls both back. A concrete store hands
/// its own scope type (wrapping whatever its transaction is) and its domain-side counterpart recognises
/// it; a foreign scope is a programming error and must throw ``DurableTxError/foreignRegistrationScope``.
public protocol DurableTxRegistrationScope: AnyObject {}

/// Runs inside the registration transaction with the minted ids, in registration order.
public typealias DurableTxRegistrationHook = (any DurableTxRegistrationScope, [DurableTxId]) throws -> Void

/// The engine's durable ledger: one row per transaction it has taken responsibility for.
///
/// It stores nothing domain-shaped. Entries are never deleted; terminal rows stay as history because a
/// domain's provenance reads must still see them.
public protocol DurableTxRepositoryProtocol: Sendable {
    /// Mints a ``DurableTxId`` per registration, inserts an entry for each, and runs `onRegister` with
    /// the ids — all inside one transaction, so a domain can write its own rows and a caller can take
    /// ownership before any other reader sees the rows. All commit or none do: a throw from `onRegister`
    /// leaves nothing behind. Each entry gets a monotonic `sequence`. Returns the ids in registration order.
    func register(
        _ registrations: [DurableTxRegistration],
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId]

    /// Inserts one row per schedule with no attempt, status ``DurableTxStatus/pendingSubmission``, and
    /// runs `onRegister` with the minted ids — the same atomicity `register` gives.
    ///
    /// A non-`nil` `scope` is an already-open write transaction to join: the rows are written through it
    /// and committed by whoever opened it, and no transaction of this store's own is opened. That is the
    /// only way to make these rows commit together with a row of another subsystem, and it is required —
    /// opening a second transaction on a shared serial writer would deadlock.
    func schedule(
        _ schedules: [DurableTxSchedule],
        in scope: (any DurableTxRegistrationScope)?,
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId]

    /// The scope-joining half of ``schedule(_:in:onRegister:)``, synchronous because its caller already
    /// is: a transport writing the row that carries a payment runs inside its store's write block, which
    /// cannot suspend. Everything this does is context work, so there is nothing to await.
    func schedule(
        _ schedules: [DurableTxSchedule],
        joining scope: any DurableTxRegistrationScope,
        onRegister: DurableTxRegistrationHook
    ) throws -> [DurableTxId]

    /// Atomically applies `verdict` iff the entry's current status still equals `expectedCurrentStatus`,
    /// its attempt is still `expectedTxHash`, and it is not terminal — the read and the write share one
    /// transaction, so neither can move between them. Returns whether it wrote. The single guarded writer
    /// of a rule verdict.
    ///
    /// Matching on the attempt is what keeps a verdict about bytes that were already proven unable to
    /// land from being written onto the rebuilt transaction that replaced them.
    @discardableResult
    func updateTxStatus(
        for id: DurableTxId,
        expectedCurrentStatus: DurableTxStatus,
        expectedTxHash: Data,
        verdict: Verdict
    ) async throws -> Bool

    /// Replaces the attempt of a transaction waiting to be built and makes it `pending`, clearing any
    /// success record the previous attempt left. Returns whether it wrote — `false` when the row was no
    /// longer waiting, which is how two builders of the same row resolve.
    func startAttempt(id: DurableTxId, attempt: DurableTxAttempt) async throws -> Bool

    /// Fails a transaction waiting to be built, for good. Returns whether it wrote.
    func abandonSubmission(id: DurableTxId) async throws -> Bool

    /// Fails every transaction of `domain` waiting to be built by `policyId`, in one write.
    ///
    /// For a policy nothing has registered: every row naming it is unbuildable whatever group it is
    /// in, so the store selects them with a predicate rather than having a caller read them all back
    /// and decide one at a time. Returns how many it failed.
    @discardableResult
    func abandonSubmissions(domain: TxDomainId, policyId: SubmissionPolicyId) async throws -> Int

    /// The policy that may build this transaction again, if it has one.
    func getSubmissionPolicy(id: DurableTxId) async throws -> SubmissionPolicy?

    /// The transactions waiting to be built, across every domain, then again on every ledger change.
    func subscribePendingSubmissions() -> AnyAsyncSequence<[ScheduledDurableTx]>

    /// The transactions waiting to be built under one policy and group, ordered by `sequence`.
    func getPendingSubmissions(
        policyId: SubmissionPolicyId,
        groupId: DurableTxGroupId?
    ) async throws -> [ScheduledDurableTx]

    /// Every entry of every domain, live and terminal, ordered by `sequence`.
    func getAllEntries() async throws -> [DurableTxEntry]

    /// Every entry of one domain that carries an attempt, live and terminal, ordered by `sequence`. The
    /// recovery pass reads this twice per domain per pass, so a store backed by a database answers it
    /// with a predicate.
    ///
    /// A transaction waiting to be built is deliberately absent: it has no bytes, no window and no
    /// inclusion for any rule to read, so nothing a pass could do would decide it.
    func getAllEntries(domain: TxDomainId) async throws -> [DurableTxEntry]

    /// The entry with this id, if any.
    func getEntry(id: DurableTxId) async throws -> DurableTxEntry?

    /// A stream of an entry's status: the current value, then every change.
    func subscribeStatus(id: DurableTxId) -> AnyAsyncSequence<DurableTxStatus>

    /// Every entry registered under `groupId` in `domain`, ordered by `sequence`.
    func getGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry]

    /// A stream of the entries registered under `groupId`: the current set, then every change, ordered
    /// by `sequence`.
    func subscribeGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]>
}

public extension DurableTxRepositoryProtocol {
    /// The entry's current status, if it exists.
    func getStatus(_ id: DurableTxId) async throws -> DurableTxStatus? {
        try await getEntry(id: id)?.status
    }
}
